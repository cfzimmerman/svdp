//! Generic HTML form extraction and re-encoding.
//!
//! ServWare's edit pages are Spring MVC forms. The safe way to update one field
//! is to submit *every* control the browser would submit, with only the intended
//! values changed. Enumerating fields by hand loses whatever the page gained
//! since the code was written: the live request form carries 50 named controls
//! and the previous hand-written builder sent 39, silently clearing the other 11.
//!
//! So we parse the form, reproduce the browser's "successful controls" rules,
//! and apply a checked overlay on top.

use scraper::ElementRef;
use scraper::Html;
use scraper::Selector;

/// One named control in the form, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Control {
    name: String,
    value: String,
    /// Whether the browser would submit this control as it currently stands.
    /// Unchecked checkboxes and radios are present in the DOM but not submitted.
    submits: bool,
    /// Checkboxes and radios can be toggled by an overlay; other controls only
    /// have their value replaced.
    toggleable: bool,
}

/// A form's named controls, in document order, duplicates preserved.
///
/// Order and duplicates both matter: ServWare's own POSTs repeat some keys, and
/// we want our request to be byte-comparable with a captured browser request.
///
/// The full control inventory is kept, not just the submitted subset, so that an
/// overlay can tell a control ServWare *removed* (a real schema change, and an
/// error) from a checkbox that merely happens to be *unchecked* (normal, and
/// exactly what marking a visit complete needs to flip).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    controls: Vec<Control>,
}

#[derive(Debug, thiserror::Error)]
pub enum FormError {
    #[error("no element matched selector `{0}`")]
    NoMatch(String),
    #[error("invalid selector `{0}`")]
    BadSelector(String),
    /// The overlay named a control the form does not have. This is the canary
    /// for ServWare renaming a field: fail loudly rather than appending a
    /// parameter the server will ignore while the real field keeps its old value.
    #[error("form has no control named `{0}` (ServWare's form may have changed)")]
    UnknownField(String),
}

impl Form {
    /// Extract the submittable controls of the first element matching `selector`.
    pub fn extract(html: &str, selector: &str) -> Result<Self, FormError> {
        let doc = Html::parse_document(html);
        let sel = Selector::parse(selector).map_err(|_| FormError::BadSelector(selector.into()))?;
        let form = doc
            .select(&sel)
            .next()
            .ok_or_else(|| FormError::NoMatch(selector.into()))?;
        Ok(Self { controls: collect_controls(form) })
    }

    /// The name/value pairs a browser would submit, in document order.
    /// Extract the first form that renders every one of `required`.
    ///
    /// Preferred over selecting by `id`: the page carries several modal forms
    /// alongside the edit form, and identifying the real one by the controls it
    /// contains survives ServWare renaming the element. (The live page uses
    /// `id="editForm"`, which is not something to depend on.)
    pub fn extract_containing(html: &str, required: &[&str]) -> Result<Self, FormError> {
        let doc = Html::parse_document(html);
        let sel = Selector::parse("form").map_err(|_| FormError::BadSelector("form".into()))?;
        for element in doc.select(&sel) {
            let candidate = Self { controls: collect_controls(element) };
            if required.iter().all(|name| candidate.contains(name)) {
                return Ok(candidate);
            }
        }
        Err(FormError::NoMatch(format!("a form containing {required:?}")))
    }

    pub fn pairs(&self) -> Vec<(String, String)> {
        self.controls
            .iter()
            .filter(|c| c.submits)
            .map(|c| (c.name.clone(), c.value.clone()))
            .collect()
    }

    /// Whether the form *renders* this control, submitted or not.
    ///
    /// This is the schema check: an unchecked checkbox still counts as present.
    pub fn contains(&self, name: &str) -> bool {
        self.controls.iter().any(|c| c.name == name)
    }

    /// The value this control would submit, or `None` if it would not submit.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.controls
            .iter()
            .find(|c| c.name == name && c.submits)
            .map(|c| c.value.as_str())
    }

    /// Replace the values of named controls, keeping every other control exactly
    /// as the server rendered it.
    ///
    /// Every key must already exist in the form; an unknown key is an error
    /// rather than an append. Setting a key that appears more than once replaces
    /// the first occurrence and drops the rest, which is what a browser does when
    /// a single logical control was rendered twice.
    pub fn overlay<'a, I>(&self, changes: I) -> Result<Self, FormError>
    where
        I: IntoIterator<Item = (&'a str, String)>,
    {
        let changes: Vec<(&str, String)> = changes.into_iter().collect();
        for (name, _) in &changes {
            if !self.contains(name) {
                return Err(FormError::UnknownField((*name).into()));
            }
        }

        let mut controls = self.controls.clone();
        for (name, value) in changes {
            let mut first = true;
            for c in controls.iter_mut().filter(|c| c.name == name) {
                if !first {
                    // A logical control rendered twice submits once.
                    c.submits = false;
                    continue;
                }
                first = false;
                if c.toggleable {
                    // For a checkbox, an overlay decides whether it is checked.
                    // Anything other than an explicit falsey value checks it, and
                    // the submitted value stays the one the page declared.
                    c.submits = !matches!(value.as_str(), "" | "false" | "off");
                } else {
                    c.value = value.clone();
                    c.submits = true;
                }
            }
        }
        Ok(Self { controls })
    }

    /// Fields whose value differs, as `(name, before, after)`.
    ///
    /// Used to prove an update changes only what was intended, and to show a
    /// human exactly what a submission will do.
    pub fn diff(&self, other: &Self) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for (name, before) in self.pairs() {
            let after = other.get(&name).unwrap_or("");
            if after != before {
                out.push((name, before, after.to_string()));
            }
        }
        // Controls the overlay newly caused to submit, e.g. a checkbox turned on.
        for (name, after) in other.pairs() {
            if self.get(&name).is_none() {
                out.push((name, String::new(), after));
            }
        }
        out
    }
}

/// Reproduce the HTML "successful controls" rules for one form element.
fn collect_controls(form: ElementRef<'_>) -> Vec<Control> {
    let any = Selector::parse("input, select, textarea").expect("static selector");
    let option = Selector::parse("option").expect("static selector");
    let mut out = Vec::new();

    for el in form.select(&any) {
        let v = el.value();
        // Disabled controls are never submitted.
        if v.attr("disabled").is_some() {
            continue;
        }
        let Some(name) = v.attr("name").filter(|n| !n.is_empty()) else {
            continue;
        };
        match v.name() {
            "textarea" => out.push(Control {
                name: name.to_string(),
                value: el.text().collect::<String>(),
                submits: true,
                toggleable: false,
            }),
            "select" => {
                let mut selected: Vec<String> = el
                    .select(&option)
                    .filter(|o| o.value().attr("selected").is_some())
                    .map(|o| option_value(o))
                    .collect();
                // With nothing marked selected, a single-select browser submits
                // the first option; a multi-select submits nothing.
                if selected.is_empty() && v.attr("multiple").is_none()
                    && let Some(first) = el.select(&option).next() {
                        selected.push(option_value(first));
                    }
                for value in selected {
                    out.push(Control {
                        name: name.to_string(),
                        value,
                        submits: true,
                        toggleable: false,
                    });
                }
            }
            _ => {
                let ty = v.attr("type").unwrap_or("text").to_ascii_lowercase();
                match ty.as_str() {
                    // Only submitted when checked.
                    // Recorded even when unchecked: present in the DOM, not submitted.
                    "checkbox" | "radio" => out.push(Control {
                        name: name.to_string(),
                        value: v.attr("value").unwrap_or("on").to_string(),
                        submits: v.attr("checked").is_some(),
                        toggleable: true,
                    }),
                    // Buttons contribute only when they are the control clicked,
                    // which the caller expresses through the overlay instead.
                    "submit" | "button" | "reset" | "image" => {}
                    // File inputs submit no useful value over urlencoded forms.
                    "file" => out.push(Control {
                        name: name.to_string(),
                        value: String::new(),
                        submits: true,
                        toggleable: false,
                    }),
                    _ => out.push(Control {
                        name: name.to_string(),
                        value: v.attr("value").unwrap_or("").to_string(),
                        submits: true,
                        toggleable: false,
                    }),
                }
            }
        }
    }
    out
}

/// An option's submitted value: its `value` attribute, else its text.
fn option_value(o: ElementRef<'_>) -> String {
    o.value()
        .attr("value")
        .map(str::to_string)
        .unwrap_or_else(|| o.text().collect::<String>().trim().to_string())
}

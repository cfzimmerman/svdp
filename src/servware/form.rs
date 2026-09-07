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

/// What an overlay is allowed to do to a control.
///
/// Checkboxes and radios are not interchangeable, which the previous single
/// `toggleable: bool` hid. A checkbox overlay decides *whether* it submits; a
/// radio overlay decides *which* member of the group submits, and the requested
/// value is the thing that chooses. Collapsing the two made
/// `overlay([("mode", "B")])` submit the first radio's value instead of `B`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Text, hidden, select, textarea: the overlay replaces the value.
    Value,
    /// Submits only when checked; the overlay decides checked or not.
    Checkbox,
    /// One of a same-named group; the overlay picks by declared value.
    Radio,
}

/// One named control in the form, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Control {
    name: String,
    value: String,
    /// Whether the browser would submit this control as it currently stands.
    /// Unchecked checkboxes and radios are present in the DOM but not submitted.
    submits: bool,
    kind: Kind,
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
    /// The overlay asked a radio group for a value none of its buttons declares.
    /// Silently submitting a different option would be a wrong write.
    #[error("radio group `{name}` has no option with value `{value}`")]
    UnknownValue { name: String, value: String },
    /// The page nests one form inside another.
    ///
    /// html5ever implements the HTML5 rule for this: the inner `<form>` start
    /// tag is **dropped**, and the first `</form>` closes the outer form. The
    /// parsed tree therefore claims the inner form's controls belong to the
    /// outer one, and silently loses every outer control that follows the inner
    /// form. Neither is recoverable after parsing, so extraction refuses rather
    /// than submitting a form it cannot vouch for.
    ///
    /// Verified against the live page (September 2026): eight forms, all
    /// siblings, so this does not fire today. It is the canary for ServWare
    /// moving a modal inside the edit form.
    #[error("the page nests forms ({source_forms} in the source, {parsed_forms} after parsing); \
             extraction cannot be trusted and this tool needs an update")]
    NestedForms { source_forms: usize, parsed_forms: usize },
}

impl Form {
    /// Extract the first form that renders every one of `required`.
    ///
    /// Identifying the form by the controls it contains, rather than by `id`,
    /// survives ServWare renaming the element. (The live page uses
    /// `id="editForm"`, which is not something to depend on.)
    ///
    /// `required` is also the schema check, so pass the **full** set of controls
    /// the caller intends to overlay. A form missing one of them is not this
    /// form, and the resulting `NoMatch` is a loud failure rather than a POST
    /// that quietly omits a field.
    pub fn extract_containing(html: &str, required: &[&str]) -> Result<Self, FormError> {
        let doc = Html::parse_document(html);
        let sel = Selector::parse("form").map_err(|_| FormError::BadSelector("form".into()))?;
        let parsed_forms = doc.select(&sel).count();

        // Nesting destroys the information this whole module depends on, and it
        // is destroyed *by the parser*, before any of our code runs -- so the
        // only place to catch it is by comparing against the source text. See
        // `FormError::NestedForms`.
        let source_forms = count_form_start_tags(html);
        if source_forms > parsed_forms {
            return Err(FormError::NestedForms { source_forms, parsed_forms });
        }

        for element in doc.select(&sel) {
            let candidate = Self { controls: collect_controls(element) };
            if required.iter().all(|name| candidate.contains(name)) {
                return Ok(candidate);
            }
        }
        Err(FormError::NoMatch(format!("a form containing {required:?}")))
    }

    /// The name/value pairs a browser would submit, in document order.
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
    /// rather than an append -- the canary for ServWare renaming a field.
    ///
    /// What the requested value means depends on the control:
    ///
    /// * **Value controls** (text, hidden, select, textarea) take it verbatim.
    ///   A name rendered more than once keeps the first occurrence and drops the
    ///   rest, which is what a browser does with a duplicated logical control.
    /// * **Checkboxes** read it as on or off. The submitted value stays the one
    ///   the page declared, because that is what the browser sends.
    /// * **Radios** are selected *by* it: the button declaring that value
    ///   submits and its siblings do not. Asking for a value no button declares
    ///   is an error, not a silent fallback to the first button in the group.
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
            let group: Vec<usize> = controls
                .iter()
                .enumerate()
                .filter(|(_, c)| c.name == name)
                .map(|(i, _)| i)
                .collect();

            match controls[group[0]].kind {
                Kind::Radio => {
                    let chosen = group.iter().copied().find(|&i| controls[i].value == value);
                    let Some(chosen) = chosen else {
                        return Err(FormError::UnknownValue { name: name.to_string(), value });
                    };
                    for &i in &group {
                        controls[i].submits = i == chosen;
                    }
                }
                Kind::Checkbox => {
                    let on = is_truthy(&value);
                    for (n, &i) in group.iter().enumerate() {
                        // Only the first of a duplicated checkbox submits.
                        controls[i].submits = on && n == 0;
                    }
                }
                Kind::Value => {
                    for (n, &i) in group.iter().enumerate() {
                        if n == 0 {
                            controls[i].value = value.clone();
                            controls[i].submits = true;
                        } else {
                            controls[i].submits = false;
                        }
                    }
                }
            }
        }
        Ok(Self { controls })
    }

    /// Fields whose submitted value differs, as `(name, before, after)`.
    ///
    /// Compares the **full multiset of submitted pairs**, not a name-to-first-
    /// value lookup. ServWare repeats some names, and resolving by first
    /// occurrence both invented differences and hid real ones -- on the screen a
    /// volunteer approves immediately before the only irreversible step. Where a
    /// name submits more than once its values are joined for display.
    pub fn diff(&self, other: &Self) -> Vec<(String, String, String)> {
        fn by_name(
            pairs: Vec<(String, String)>,
        ) -> std::collections::BTreeMap<String, Vec<String>> {
            let mut out: std::collections::BTreeMap<String, Vec<String>> = Default::default();
            for (name, value) in pairs {
                out.entry(name).or_default().push(value);
            }
            out
        }
        let before = by_name(self.pairs());
        let after = by_name(other.pairs());

        let mut names: Vec<&String> = before.keys().chain(after.keys()).collect();
        names.sort();
        names.dedup();

        let render = |v: Option<&Vec<String>>| v.map(|v| v.join(", ")).unwrap_or_default();
        names
            .into_iter()
            .filter(|n| before.get(*n) != after.get(*n))
            .map(|n| (n.clone(), render(before.get(n)), render(after.get(n))))
            .collect()
    }
}

/// Whether an overlay value asks for a checkbox to be checked.
///
/// The falsey set used to be `"" | "false" | "off"` only, so `"0"` and `"no"`
/// turned a box **on**.
fn is_truthy(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "false" | "off" | "0" | "no" | "n" | "unchecked"
    )
}

/// Count `<form` start tags in the source, skipping comments and `<script>` /
/// `<style>` bodies, where the same text is not markup and the parser would not
/// create an element.
///
/// Used only to detect that html5ever dropped a nested form; see
/// [`FormError::NestedForms`].
fn count_form_start_tags(html: &str) -> usize {
    let lower = html.to_ascii_lowercase();
    let mut i = 0usize;
    let mut count = 0usize;

    let skip_past = |from: usize, needle: &str| -> usize {
        lower[from..]
            .find(needle)
            .map(|k| from + k + needle.len())
            .unwrap_or(lower.len())
    };

    while i < lower.len() {
        let Some(next) = lower[i..].find('<') else { break };
        let at = i + next;
        let rest = &lower[at..];
        if rest.starts_with("<!--") {
            i = skip_past(at, "-->");
        } else if rest.starts_with("<script") {
            i = skip_past(at, "</script>");
        } else if rest.starts_with("<style") {
            i = skip_past(at, "</style>");
        } else if rest.starts_with("<form")
            && rest[5..]
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || c == '>' || c == '/')
        {
            count += 1;
            i = at + 5;
        } else {
            i = at + 1;
        }
    }
    count
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
                kind: Kind::Value,
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
                        kind: Kind::Value,
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
                        kind: if ty == "radio" { Kind::Radio } else { Kind::Checkbox },
                    }),
                    // Buttons contribute only when they are the control clicked,
                    // which the caller expresses through the overlay instead.
                    "submit" | "button" | "reset" | "image" => {}
                    // File inputs submit no useful value over urlencoded forms.
                    "file" => out.push(Control {
                        name: name.to_string(),
                        value: String::new(),
                        submits: true,
                        kind: Kind::Value,
                    }),
                    _ => out.push(Control {
                        name: name.to_string(),
                        value: v.attr("value").unwrap_or("").to_string(),
                        submits: true,
                        kind: Kind::Value,
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

use std::env;

use anyhow::Context;
use secrecy::SecretString;
use tracing::debug;

pub struct Credentials {
    pub username: String,
    pub password: SecretString,
}

/// Load credentials from env vars or prompt interactively.
///
/// Checks `SVDP_USERNAME` and `SVDP_PASSWORD` first.
/// Falls back to interactive prompts if either is missing.
pub fn get_credentials() -> anyhow::Result<Credentials> {
    let username = match env::var("SVDP_USERNAME") {
        Ok(u) => {
            debug!("using SVDP_USERNAME from environment");
            u
        }
        Err(_) => {
            eprint!("Username: ");
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .context("failed to read username")?;
            input.trim().to_string()
        }
    };

    let password: SecretString = match env::var("SVDP_PASSWORD") {
        Ok(p) => {
            debug!("using SVDP_PASSWORD from environment");
            p.into()
        }
        Err(_) => {
            let p = rpassword::prompt_password("Password: ").context("failed to read password")?;
            p.into()
        }
    };

    Ok(Credentials { username, password })
}

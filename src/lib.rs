pub mod api;
pub mod nativity;

use anyhow::Context;
use secrecy::SecretString;

pub struct Credentials {
    pub username: String,
    pub password: SecretString,
}

impl Credentials {
    pub const USER_ENV: &str = "SERVWARE_USER";
    pub const PASS_ENV: &str = "SERVWARE_PASS";

    /// Interactively requests ServWare credentials from the user.
    pub fn prompt(use_env: bool) -> anyhow::Result<Credentials> {
        let mut buf = String::new();

        if use_env {
            tracing::info!("checking env for {} and {}", Self::USER_ENV, Self::PASS_ENV);
            let _ = dotenvy::dotenv();
        }

        let username = match std::env::var(Self::USER_ENV) {
            Ok(from_env) => from_env.trim().to_string(),
            Err(_) => {
                eprint!("ServWare username: ");
                std::io::stdin()
                    .read_line(&mut buf)
                    .context("failed to read username")?;
                buf.trim().to_string()
            }
        };

        let password = match std::env::var(Self::PASS_ENV) {
            Ok(from_env) => SecretString::new(from_env.trim().to_string().into_boxed_str()),
            Err(_) => rpassword::prompt_password("ServWare password: ")
                .context("failed to read password")?
                .into(),
        };

        Ok(Credentials { username, password })
    }
}

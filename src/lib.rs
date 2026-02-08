pub mod api;

use anyhow::Context;
use secrecy::SecretString;

pub struct Credentials {
    pub username: String,
    pub password: SecretString,
}

/// Interactively requests ServWare credentials from the user.
pub fn get_credentials() -> anyhow::Result<Credentials> {
    let mut buf = String::new();
    let username = {
        eprint!("ServWare username: ");
        std::io::stdin()
            .read_line(&mut buf)
            .context("failed to read username")?;
        buf.trim().to_string()
    };

    let password: SecretString = rpassword::prompt_password("Password: ")
        .context("failed to read password")?
        .into();

    Ok(Credentials { username, password })
}

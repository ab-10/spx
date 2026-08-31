use anyhow::{Result, bail};
use std::thread;
use std::time::Duration;

use crate::commands::api;
use crate::credentials::Credentials;
use crate::ui;

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const POLL_ATTEMPTS: usize = 60;

pub fn subscribe(verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();

    if verbose {
        ui::verbose(&format!("GET {}/billing/", api_url.trim_end_matches('/')));
    }
    if api::billing_status(&api_url, &creds.token)?.has_access {
        ui::success("Subscription active");
        return Ok(());
    }

    if verbose {
        ui::verbose(&format!(
            "POST {}/billing/checkout/",
            api_url.trim_end_matches('/')
        ));
    }
    let checkout = api::billing_checkout(&api_url, &creds.token)?;

    ui::info("Complete the purchase on the Stripe Checkout page in your browser");
    println!("{}", checkout.url);
    if let Err(err) = open::that(&checkout.url) {
        ui::warn(&format!("could not open browser: {err}"));
    }

    ui::info("Waiting for subscription to activate...");
    for _ in 0..POLL_ATTEMPTS {
        thread::sleep(POLL_INTERVAL);
        if api::billing_status(&api_url, &creds.token)?.has_access {
            ui::success("Subscription active");
            return Ok(());
        }
    }

    bail!("subscription is not active yet. If you completed checkout, try `spx subscribe` again.")
}

# flaremail-rs

Rust SDK for Cloudflare Email Service.

## Install

```toml
[dependencies]
flaremail-rs = "0.1"
```

## Quick Start

```rust
use flaremail_rs::{Email, SendEmail};

#[tokio::main]
async fn main() -> Result<(), flaremail_rs::Error> {
    let email = Email::new(std::env::var("CLOUDFLARE_API_TOKEN").unwrap())
        .with_account_id(std::env::var("CLOUDFLARE_ACCOUNT_ID").unwrap());

    let sent = email
        .emails()
        .send(
            SendEmail::new("Acme <noreply@example.com>", "user@example.com", "Hello world")
                .html("<strong>It works!</strong>"),
        )
        .await?;

    println!("sent {}", sent.id);
    Ok(())
}
```

## API

```rust
let email = Email::new(api_key).with_account_id(account_id);

email.emails().send(message).await?;
email.emails().create(message).await?;
email
    .emails()
    .send_with_options(message, SendOptions::new().idempotency_key("welcome/user-123"))
    .await?;
```

## Account ID

Cloudflare's REST endpoint requires an account ID. Pass it with `with_account_id`, set `CLOUDFLARE_ACCOUNT_ID` or `CF_ACCOUNT_ID`, or let the crate auto-discover it when the API token can read exactly one Cloudflare account.

## Attachments

Attachment content should be base64 encoded.

```rust
use flaremail_rs::{Attachment, Email, SendEmail};

let message = SendEmail::new("Invoices <invoices@example.com>", "customer@example.com", "Your invoice")
    .html("<p>Invoice attached.</p>")
    .attachment(Attachment::new(
        "JVBERi0xLjQKJeLjz9MK...",
        "invoice.pdf",
        "application/pdf",
    ));
```

## Development

```sh
cargo fmt
cargo test
cargo check
```

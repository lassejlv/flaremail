# flaremail

Tiny TypeScript SDK for Cloudflare Email Service.

## Install

```sh
bun add flaremail
```

For local development in this repository:

```sh
cd cfsend
bun install
bun test
bun run build
```

## Quick Start

```ts
import { Email } from 'flaremail'

const email = new Email(process.env.CLOUDFLARE_API_TOKEN, {
  accountId: process.env.CLOUDFLARE_ACCOUNT_ID,
})

const { data, error } = await email.emails.send({
  from: 'Acme <noreply@example.com>',
  to: ['user@example.com'],
  subject: 'Hello world',
  html: '<strong>It works!</strong>',
})

if (error) {
  console.error(error.message)
} else {
  console.log(data.id)
}
```

## API

The main API keeps the familiar `emails.send` flow:

```ts
const email = new Email(apiKey, { accountId })
await email.emails.send(payload, { idempotencyKey })
await email.emails.create(payload, { idempotencyKey })
```

Both methods resolve to this response object:

```ts
type EmailResponse<T> =
  | { data: T; error: null }
  | { data: null; error: EmailError }
```

## Cloudflare account ID

Cloudflare's REST endpoint requires an account ID. Pass it with `accountId`, set `CLOUDFLARE_ACCOUNT_ID` or `CF_ACCOUNT_ID`, or let flaremail auto-discover it when the API token can read exactly one Cloudflare account.

```ts
const email = new Email(process.env.CLOUDFLARE_API_TOKEN, {
  accountId: '<cloudflare-account-id>',
})
```

The API token needs permission to send email. If you rely on account auto-discovery, it also needs account read access.

## Supported fields

Supported fields are `from`, `to`, `subject`, `html`, `text`, `cc`, `bcc`, `replyTo`, `reply_to`, `headers`, and `attachments`.

Cloudflare Email Service does not currently support `react`, `scheduledAt`, `tags`, `topic_id`, or `template` fields through this adapter.

## Attachments

Attachment content should be base64 encoded, matching Cloudflare's REST API.

```ts
await email.emails.send({
  from: 'Invoices <invoices@example.com>',
  to: 'customer@example.com',
  subject: 'Your invoice',
  html: '<p>Invoice attached.</p>',
  attachments: [
    {
      filename: 'invoice.pdf',
      content: 'JVBERi0xLjQKJeLjz9MK...',
      type: 'application/pdf',
      disposition: 'attachment',
    },
  ],
})
```

## Custom fetch

Pass a custom `fetch` for tests or non-standard runtimes:

```ts
const email = new Email('cf-token', {
  accountId: 'account-id',
  fetch: myFetch,
})
```

## Build

This package is bundled with tsdown and emits ESM, CJS, and declaration files into `dist`.

```sh
bun run typecheck
bun test
bun run build
```

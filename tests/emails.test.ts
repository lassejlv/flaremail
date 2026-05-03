import { describe, expect, test } from 'bun:test'
import { Email, EmailError } from '../src/index'

type FetchCall = {
  url: string
  init: RequestInit
}

function createFetchMock(response: unknown, options: { ok?: boolean; status?: number; statusText?: string } = {}) {
  const calls: FetchCall[] = []
  const fetchMock = async (url: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ url: String(url), init: init ?? {} })

    return new Response(JSON.stringify(response), {
      status: options.status ?? (options.ok === false ? 400 : 200),
      statusText: options.statusText,
      headers: { 'Content-Type': 'application/json' },
    })
  }

  return { calls, fetchMock: fetchMock as typeof fetch }
}

describe('emails.send', () => {
  test('sends an emails.send payload to the Cloudflare REST endpoint', async () => {
    const { calls, fetchMock } = createFetchMock({
      success: true,
      errors: [],
      messages: [],
      result: { messageId: 'msg_123' },
    })
    const email = new Email('cf-token', {
      accountId: 'account-123',
      baseUrl: 'https://api.example.test/client/v4/',
      fetch: fetchMock,
    })

    const result = await email.emails.send(
      {
        from: 'Acme <noreply@example.com>',
        to: ['user@example.com'],
        subject: 'Hello world',
        html: '<strong>It works!</strong>',
        text: 'It works!',
        cc: 'cc@example.com',
        bcc: ['bcc@example.com'],
        replyTo: 'reply@example.com',
        headers: { 'X-Test': 'true' },
        attachments: [
          {
            content: 'SGVsbG8=',
            filename: 'hello.txt',
            type: 'text/plain',
            contentId: 'hello-file',
          },
        ],
      },
      { idempotencyKey: 'welcome/user-123' },
    )

    expect(result.error).toBeNull()
    expect(result.data?.id).toBe('msg_123')
    expect(calls).toHaveLength(1)
    expect(calls[0].url).toBe('https://api.example.test/client/v4/accounts/account-123/email/sending/send')
    expect(calls[0].init.method).toBe('POST')
    expect(calls[0].init.headers).toEqual({
      Authorization: 'Bearer cf-token',
      'Content-Type': 'application/json',
      'Idempotency-Key': 'welcome/user-123',
    })
    expect(JSON.parse(String(calls[0].init.body))).toEqual({
      from: { address: 'noreply@example.com', name: 'Acme' },
      to: ['user@example.com'],
      subject: 'Hello world',
      html: '<strong>It works!</strong>',
      text: 'It works!',
      cc: 'cc@example.com',
      bcc: ['bcc@example.com'],
      reply_to: 'reply@example.com',
      headers: { 'X-Test': 'true' },
      attachments: [
        {
          content: 'SGVsbG8=',
          filename: 'hello.txt',
          type: 'text/plain',
          disposition: 'attachment',
          content_id: 'hello-file',
        },
      ],
    })
  })

  test('returns Cloudflare API errors in the SDK response shape', async () => {
    const { fetchMock } = createFetchMock(
      {
        success: false,
        errors: [{ code: 1234, message: 'Invalid from address' }],
        messages: [],
        result: null,
      },
      { ok: false, status: 400 },
    )
    const email = new Email('cf-token', { accountId: 'account-123', fetch: fetchMock })

    const result = await email.emails.send({
      from: 'noreply@example.com',
      to: 'user@example.com',
      subject: 'Hello world',
      html: '<strong>It works!</strong>',
    })

    expect(result.data).toBeNull()
    expect(result.error).toBeInstanceOf(EmailError)
    expect(result.error?.message).toBe('Invalid from address')
    expect(result.error?.statusCode).toBe(400)
    expect(result.error?.code).toBe(1234)
  })

  test('auto-discovers account ID when the token has exactly one account', async () => {
    const responses = [
      { success: true, errors: [], messages: [], result: [{ id: 'account-123' }] },
      { success: true, errors: [], messages: [], result: { messageId: 'msg_123' } },
    ]
    const calls: FetchCall[] = []
    const fetchMock = (async (url: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ url: String(url), init: init ?? {} })
      return Response.json(responses.shift())
    }) as typeof fetch
    const email = new Email('cf-token', { fetch: fetchMock })

    const result = await email.emails.create({
      from: 'noreply@example.com',
      to: 'user@example.com',
      subject: 'Hello world',
      text: 'It works!',
    })

    expect(result.error).toBeNull()
    expect(result.data?.id).toBe('msg_123')
    expect(calls.map((call) => call.url)).toEqual([
      'https://api.cloudflare.com/client/v4/accounts',
      'https://api.cloudflare.com/client/v4/accounts/account-123/email/sending/send',
    ])
  })

  test('rejects unsupported fields before making a request', async () => {
    const { calls, fetchMock } = createFetchMock({ success: true, errors: [], messages: [], result: {} })
    const email = new Email('cf-token', { accountId: 'account-123', fetch: fetchMock })

    const result = await email.emails.send({
      from: 'noreply@example.com',
      to: 'user@example.com',
      subject: 'Hello world',
      html: '<strong>It works!</strong>',
      scheduledAt: 'in 5 minutes',
    })

    expect(result.data).toBeNull()
    expect(result.error?.message).toBe('Cloudflare Email Service does not support: scheduledAt')
    expect(calls).toHaveLength(0)
  })
})

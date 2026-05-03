export type FetchLike = typeof fetch

export type EmailAddress = string | { address: string; name?: string } | { email: string; name?: string }

export type EmailAttachment = {
  content: string
  filename: string
  type?: string
  disposition?: 'attachment' | 'inline'
  content_id?: string
  contentId?: string
}

export type CreateEmailOptions = {
  idempotencyKey?: string
}

export type CreateEmailRequestOptions = CreateEmailOptions

export type CreateEmailPayload = {
  from: EmailAddress
  to: string | string[]
  subject: string
  bcc?: string | string[]
  cc?: string | string[]
  replyTo?: string | string[]
  reply_to?: string | string[]
  html?: string
  text?: string
  headers?: Record<string, string>
  attachments?: EmailAttachment[]
  scheduledAt?: string
  tags?: Array<{ name: string; value: string }>
  topic_id?: string
  template?: { id: string; variables?: Record<string, string | number> }
  react?: unknown
}

export type CreateEmailResponse = {
  id: string
}

export type EmailSuccess<T> = {
  data: T
  error: null
}

export type EmailFailure = {
  data: null
  error: EmailError
}

export type EmailResponse<T> = EmailSuccess<T> | EmailFailure

export type EmailOptions = {
  accountId?: string
  baseUrl?: string
  fetch?: FetchLike
}

export class EmailError extends Error {
  readonly name = 'EmailError'
  readonly statusCode?: number
  readonly code?: string | number
  readonly details?: unknown

  constructor(message: string, options: { statusCode?: number; code?: string | number; details?: unknown } = {}) {
    super(message)
    this.statusCode = options.statusCode
    this.code = options.code
    this.details = options.details
  }
}

export class Email {
  readonly emails: Emails

  #apiKey: string
  #accountId?: string
  #baseUrl: string
  #fetch: FetchLike

  constructor(apiKey = readEnv('CLOUDFLARE_API_TOKEN') ?? readEnv('CF_API_TOKEN') ?? '', options: EmailOptions = {}) {
    this.#apiKey = apiKey
    this.#accountId = options.accountId ?? readEnv('CLOUDFLARE_ACCOUNT_ID') ?? readEnv('CF_ACCOUNT_ID')
    this.#baseUrl = (options.baseUrl ?? 'https://api.cloudflare.com/client/v4').replace(/\/$/, '')
    this.#fetch = options.fetch ?? globalThis.fetch.bind(globalThis)
    this.emails = new Emails(this)
  }

  async request<T>(path: string, init: RequestInit = {}): Promise<EmailResponse<T>> {
    if (!this.#apiKey) {
      return failure('Missing Cloudflare API token')
    }

    try {
      const response = await this.#fetch(`${this.#baseUrl}${path}`, {
        ...init,
        headers: {
          Authorization: `Bearer ${this.#apiKey}`,
          ...init.headers,
        },
      })

      const json = await response.json().catch(() => null)

      if (!response.ok || !isCloudflareSuccess(json)) {
        return failure(getCloudflareErrorMessage(json) ?? response.statusText, {
          statusCode: response.status,
          code: getCloudflareErrorCode(json),
          details: json,
        })
      }

      return { data: json.result as T, error: null }
    } catch (error) {
      return failure(error instanceof Error ? error.message : 'Request failed', { details: error })
    }
  }

  async accountId(): Promise<EmailResponse<string>> {
    if (this.#accountId) {
      return { data: this.#accountId, error: null }
    }

    const accounts = await this.request<Array<{ id: string }>>('/accounts')
    if (accounts.error) return accounts

    if (accounts.data.length !== 1) {
      return failure(
        accounts.data.length === 0
          ? 'No Cloudflare accounts found for this API token'
          : 'Multiple Cloudflare accounts found; pass accountId in the constructor options',
      )
    }

    this.#accountId = accounts.data[0].id
    return { data: this.#accountId, error: null }
  }
}

export class Emails {
  #client: Email

  constructor(client: Email) {
    this.#client = client
  }

  async send(payload: CreateEmailPayload, options: CreateEmailOptions = {}): Promise<EmailResponse<CreateEmailResponse>> {
    const accountId = await this.#client.accountId()
    if (accountId.error) return accountId

    const body = toCloudflareEmailPayload(payload)
    if (body.error) return body

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    }

    if (options.idempotencyKey) {
      headers['Idempotency-Key'] = options.idempotencyKey
    }

    const response = await this.#client.request<Record<string, unknown>>(
      `/accounts/${accountId.data}/email/sending/send`,
      {
        method: 'POST',
        headers,
        body: JSON.stringify(body.data),
      },
    )

    if (response.error) return response

    return {
      data: { id: getEmailId(response.data) },
      error: null,
    }
  }

  create(payload: CreateEmailPayload, options?: CreateEmailOptions) {
    return this.send(payload, options)
  }
}

function toCloudflareEmailPayload(payload: CreateEmailPayload): EmailResponse<Record<string, unknown>> {
  if (!payload.from) return failure('from is required')
  if (!payload.to) return failure('to is required')
  if (!payload.subject) return failure('subject is required')
  if (!payload.html && !payload.text) return failure('Either html or text is required')

  const unsupported = [
    payload.react !== undefined ? 'react' : undefined,
    payload.scheduledAt !== undefined ? 'scheduledAt' : undefined,
    payload.tags !== undefined ? 'tags' : undefined,
    payload.topic_id !== undefined ? 'topic_id' : undefined,
    payload.template !== undefined ? 'template' : undefined,
  ].filter(Boolean)

  if (unsupported.length > 0) {
    return failure(`Cloudflare Email Service does not support: ${unsupported.join(', ')}`)
  }

  const body: Record<string, unknown> = {
    from: normalizeAddress(payload.from),
    to: payload.to,
    subject: payload.subject,
  }

  if (payload.html !== undefined) body.html = payload.html
  if (payload.text !== undefined) body.text = payload.text
  if (payload.cc !== undefined) body.cc = payload.cc
  if (payload.bcc !== undefined) body.bcc = payload.bcc
  if (payload.headers !== undefined) body.headers = payload.headers
  if (payload.attachments !== undefined) body.attachments = payload.attachments.map(normalizeAttachment)

  const replyTo = payload.reply_to ?? payload.replyTo
  if (replyTo !== undefined) body.reply_to = Array.isArray(replyTo) ? replyTo.join(', ') : replyTo

  return { data: body, error: null }
}

function normalizeAddress(address: EmailAddress) {
  if (typeof address !== 'string') {
    if ('address' in address) return address
    return { address: address.email, name: address.name }
  }

  const match = /^(.+?)\s*<([^>]+)>$/.exec(address)
  if (!match) return address

  return {
    address: match[2].trim(),
    name: match[1].trim().replace(/^['"]|['"]$/g, ''),
  }
}

function normalizeAttachment(attachment: EmailAttachment) {
  return {
    content: attachment.content,
    filename: attachment.filename,
    type: attachment.type ?? 'application/octet-stream',
    disposition: attachment.disposition ?? 'attachment',
    ...(attachment.content_id || attachment.contentId
      ? { content_id: attachment.content_id ?? attachment.contentId }
      : {}),
  }
}

function getEmailId(result: Record<string, unknown>) {
  const id = result.id ?? result.messageId ?? result.message_id
  if (typeof id === 'string') return id

  const delivered = Array.isArray(result.delivered) ? result.delivered : []
  const queued = Array.isArray(result.queued) ? result.queued : []
  return [...delivered, ...queued].filter((value): value is string => typeof value === 'string').join(',')
}

function isCloudflareSuccess(value: unknown): value is { success: true; result: unknown } {
  return isRecord(value) && value.success === true && 'result' in value
}

function getCloudflareErrorMessage(value: unknown) {
  if (!isRecord(value) || !Array.isArray(value.errors) || value.errors.length === 0) return undefined
  const firstError = value.errors[0]
  if (!isRecord(firstError)) return undefined
  return typeof firstError.message === 'string' ? firstError.message : undefined
}

function getCloudflareErrorCode(value: unknown) {
  if (!isRecord(value) || !Array.isArray(value.errors) || value.errors.length === 0) return undefined
  const firstError = value.errors[0]
  if (!isRecord(firstError)) return undefined
  return typeof firstError.code === 'string' || typeof firstError.code === 'number' ? firstError.code : undefined
}

function failure(message: string, options?: ConstructorParameters<typeof EmailError>[1]): EmailFailure {
  return { data: null, error: new EmailError(message, options) }
}

function readEnv(name: string) {
  const processLike = globalThis as typeof globalThis & { process?: { env?: Record<string, string | undefined> } }
  return processLike.process?.env?.[name]
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

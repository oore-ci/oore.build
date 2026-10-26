import { z } from 'zod'

import type { User, UserRole } from '@oore/client/models'

const importRoleSchema = z.enum(['owner', 'admin', 'developer', 'qa_viewer'])
const EMAIL_SCHEMA = z.email()
const MAX_IMPORT_ROWS = 200

export interface CsvUserImportRow {
  email: string
  role: UserRole
  row: number
}

export interface CsvUserImportError {
  message: string
  row?: number
}

export interface CsvUserImportResult {
  errors: Array<CsvUserImportError>
  rows: Array<CsvUserImportRow>
}

function parseCsvRecords(source: string): Array<Array<string>> {
  const records: Array<Array<string>> = []
  let field = ''
  let record: Array<string> = []
  let quoted = false

  for (let index = 0; index < source.length; index += 1) {
    const character = source[index]
    const nextCharacter = source[index + 1]

    if (quoted) {
      if (character === '"' && nextCharacter === '"') {
        field += '"'
        index += 1
      } else if (character === '"') {
        quoted = false
      } else {
        field += character
      }
      continue
    }

    if (character === '"') {
      quoted = true
    } else if (character === ',') {
      record.push(field)
      field = ''
    } else if (character === '\n') {
      record.push(field)
      records.push(record)
      field = ''
      record = []
    } else if (character !== '\r') {
      field += character
    }
  }

  record.push(field)
  records.push(record)
  return records
}

function escapeCsvField(value: string): string {
  return /[",\n\r]/.test(value) ? `"${value.replaceAll('"', '""')}"` : value
}

export function parseUserCsv(source: string): CsvUserImportResult {
  const records = parseCsvRecords(source.replace(/^\uFEFF/, '')).filter(
    (record) => record.some((field) => field.trim()),
  )
  const header = records.shift()?.map((field) => field.trim().toLowerCase())

  if (!header) {
    return { rows: [], errors: [{ message: 'The CSV file is empty.' }] }
  }

  const emailIndex = header.indexOf('email')
  const roleIndex = header.indexOf('role')
  if (emailIndex === -1 || roleIndex === -1) {
    return {
      rows: [],
      errors: [
        { message: 'The CSV header must contain email and role columns.' },
      ],
    }
  }

  if (records.length > MAX_IMPORT_ROWS) {
    return {
      rows: [],
      errors: [
        {
          message: `A CSV import can contain at most ${MAX_IMPORT_ROWS} users.`,
        },
      ],
    }
  }

  const seenEmails = new Set<string>()
  const rows: Array<CsvUserImportRow> = []
  const errors: Array<CsvUserImportError> = []

  for (const [index, record] of records.entries()) {
    const row = index + 2
    const email = (record[emailIndex] ?? '').trim()
    const role = (record[roleIndex] ?? '').trim().toLowerCase()
    const normalizedEmail = email.toLowerCase()

    if (!z.validate(EMAIL_SCHEMA, email)) {
      errors.push({ row, message: 'Enter a valid email address.' })
      continue
    }
    const parsedRole = importRoleSchema.safeParse(role)
    if (!parsedRole.success) {
      errors.push({
        row,
        message: 'Use admin, developer, or qa_viewer as the role.',
      })
      continue
    }
    if (seenEmails.has(normalizedEmail)) {
      errors.push({
        row,
        message: 'The email address is repeated in this file.',
      })
      continue
    }

    seenEmails.add(normalizedEmail)
    rows.push({ email, role: parsedRole.data, row })
  }

  return { rows, errors }
}

export function usersCsv(users: Array<User>): string {
  const lines = users.map((user) =>
    [user.email, user.role, user.status].map(escapeCsvField).join(','),
  )
  return ['email,role,status', ...lines].join('\n')
}

export function downloadUsersCsv(users: Array<User>) {
  const blob = new Blob([usersCsv(users)], { type: 'text/csv;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = 'oore-users.csv'
  anchor.click()
  URL.revokeObjectURL(url)
}

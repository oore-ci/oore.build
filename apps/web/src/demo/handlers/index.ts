import { setupHandlers } from './setup'
import { authHandlers } from './auth'
import { userHandlers } from './users'
import { projectHandlers } from './projects'
import { pipelineHandlers } from './pipelines'
import { buildHandlers } from './builds'
import { artifactHandlers } from './artifacts'
import { integrationHandlers } from './integrations'
import { runnerHandlers } from './runners'
import { settingsHandlers } from './settings'
import { notificationHandlers } from './notifications'
import { retentionHandlers } from './retention'
import { auditLogHandlers } from './audit-logs'

export const allHandlers = [
  ...setupHandlers,
  ...authHandlers,
  ...userHandlers,
  ...projectHandlers,
  ...pipelineHandlers,
  ...buildHandlers,
  ...artifactHandlers,
  ...integrationHandlers,
  ...runnerHandlers,
  ...settingsHandlers,
  ...notificationHandlers,
  ...retentionHandlers,
  ...auditLogHandlers,
]

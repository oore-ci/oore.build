import { setupHandlers } from './setup'
import {
  authGuardHandlers,
  authHandlers,
  demoReadOnlyGuardHandlers,
} from './auth'
import { userHandlers } from './users'
import { projectHandlers } from './projects'
import { pipelineHandlers } from './pipelines'
import { buildHandlers } from './builds'
import { releaseHandlers } from './releases'
import { artifactHandlers } from './artifacts'
import { integrationHandlers } from './integrations'
import { runnerHandlers } from './runners'
import { settingsHandlers } from './settings'
import { notificationHandlers } from './notifications'
import { retentionHandlers } from './retention'
import { auditLogHandlers } from './audit-logs'
import { apiTokenHandlers } from './api-tokens'
import { secretHandlersList } from './secrets'
import { storeCredentialHandlers } from './store-credentials'

export const allHandlers = [
  ...demoReadOnlyGuardHandlers,
  ...authGuardHandlers,
  ...setupHandlers,
  ...authHandlers,
  ...userHandlers,
  ...projectHandlers,
  ...pipelineHandlers,
  ...buildHandlers,
  ...releaseHandlers,
  ...artifactHandlers,
  ...integrationHandlers,
  ...runnerHandlers,
  ...settingsHandlers,
  ...notificationHandlers,
  ...retentionHandlers,
  ...auditLogHandlers,
  ...apiTokenHandlers,
  ...secretHandlersList,
  ...storeCredentialHandlers,
]

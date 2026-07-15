import { USER_IDS, ago } from '../seed'
import type { User } from '@/lib/types'

export const demoUsers: Array<User> = [
  {
    id: USER_IDS.owner,
    email: 'demo+owner@oore.build',
    display_name: 'Alex Chen',
    role: 'owner',
    status: 'active',
    created_at: ago(86400 * 90),
    updated_at: ago(3600),
  },
  {
    id: USER_IDS.admin,
    email: 'demo+admin@oore.build',
    display_name: 'Jamie Park',
    role: 'admin',
    status: 'active',
    created_at: ago(86400 * 60),
    updated_at: ago(86400 * 2),
  },
  {
    id: USER_IDS.developer,
    email: 'demo+developer@oore.build',
    display_name: 'Morgan Lee',
    role: 'developer',
    status: 'active',
    created_at: ago(86400 * 30),
    updated_at: ago(86400 * 5),
  },
  {
    id: USER_IDS.qaViewer,
    email: 'demo+qa@oore.build',
    display_name: 'Taylor Ruiz',
    role: 'qa_viewer',
    status: 'active',
    created_at: ago(86400 * 14),
    updated_at: ago(86400 * 7),
  },
  {
    id: USER_IDS.invited,
    email: 'sam@example.com',
    role: 'developer',
    status: 'invited',
    created_at: ago(3600 * 2),
    updated_at: ago(3600 * 2),
  },
]

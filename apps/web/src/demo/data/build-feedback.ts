import { BUILD_IDS, ago } from '../seed'
import type { BuildFeedback } from '@oore/client/models'

export const demoBuildFeedback = {
  [BUILD_IDS.succeeded1]: [
    {
      id: 'feedback-demo-001',
      build_id: BUILD_IDS.succeeded1,
      author_email: 'demo+qa@oore.build',
      author_name: 'Taylor Ruiz',
      message:
        'Checkout is much faster now. Saved addresses load right away and the new card error reads well.',
      device: 'iPhone',
      os_version: 'iOS 18.1',
      app_version: '1.4.1+85',
      has_screenshot: false,
      created_at: ago(5400),
    },
    {
      id: 'feedback-demo-002',
      build_id: BUILD_IDS.succeeded1,
      author_email: 'demo+qa@oore.build',
      author_name: 'Taylor Ruiz',
      message:
        'The Pay button overlaps the total on small screens after I apply a promo code.\nSteps: add 2 items, apply SPRING10, open the cart.',
      device: 'Pixel 7a',
      os_version: 'Android 15',
      app_version: '1.4.1+85',
      has_screenshot: true,
      created_at: ago(1800),
    },
  ],
} satisfies Partial<Record<string, Array<BuildFeedback>>>

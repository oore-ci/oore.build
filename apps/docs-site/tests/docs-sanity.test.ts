import fs from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

describe('Documentation Sanity Tests', () => {
  const docsDir = path.resolve(__dirname, '../docs/getting-started');

  const criticalPages = [
    'install.md',
    'public-alpha.md',
    'hosted-ui-onboarding.md',
    'known-limitations.md',
    'clean-reinstall.md',
    'issue-report-checklist.md'
  ];

  it('should have all critical getting-started pages', () => {
    criticalPages.forEach((page) => {
      const filePath = path.join(docsDir, page);
      const exists = fs.existsSync(filePath);
      expect(exists, `Page ${page} should exist in ${docsDir}`).toBe(true);
    });
  });
});

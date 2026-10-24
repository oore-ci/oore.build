import { BUILD_IDS } from '../seed'
import type { BuildAnalysisResponse } from '@oore/client/models'

/** `dart analyze` summaries for builds that show one in the demo. */
export const demoBuildAnalyses = new Map<string, BuildAnalysisResponse>([
  [
    BUILD_IDS.succeeded1,
    {
      base_branch: 'main',
      baseline: {
        build_id: BUILD_IDS.succeeded4,
        build_number: 138,
        branch: 'main',
        counts: { errors: 0, warnings: 1, infos: 9 },
      },
      analysis: {
        counts: { errors: 0, warnings: 3, infos: 7 },
        recorded_at: 1_761_300_000,
        diagnostics: [
          {
            severity: 'warning',
            code: 'unused_import',
            message: "Unused import: 'package:flutter_shop/legacy/cart.dart'.",
            file: 'lib/checkout/checkout_page.dart',
            line: 4,
            column: 8,
          },
          {
            severity: 'warning',
            code: 'dead_code',
            message: 'Dead code.',
            file: 'lib/checkout/payment_errors.dart',
            line: 57,
            column: 7,
          },
          {
            severity: 'warning',
            code: 'unnecessary_null_comparison',
            message:
              "The operand can't be 'null', so the condition is always 'true'.",
            file: 'lib/addresses/address_book.dart',
            line: 112,
            column: 13,
          },
          {
            severity: 'info',
            code: 'prefer_const_constructors',
            message: "Use 'const' with the constructor to improve performance.",
            file: 'lib/checkout/checkout_page.dart',
            line: 88,
            column: 16,
          },
          {
            severity: 'info',
            code: 'use_build_context_synchronously',
            message: "Don't use 'BuildContext's across async gaps.",
            file: 'lib/addresses/address_form.dart',
            line: 140,
            column: 9,
          },
        ],
      },
    },
  ],
  [
    BUILD_IDS.failed1,
    {
      base_branch: 'main',
      baseline: null,
      analysis: {
        counts: { errors: 1, warnings: 0, infos: 2 },
        recorded_at: 1_761_300_000,
        diagnostics: [
          {
            severity: 'error',
            code: 'undefined_identifier',
            message: "Undefined name 'sessoin'.",
            file: 'lib/auth/login_controller.dart',
            line: 42,
            column: 5,
          },
          {
            severity: 'info',
            code: 'avoid_print',
            message: "Don't invoke 'print' in production code.",
            file: 'lib/auth/login_controller.dart',
            line: 61,
            column: 3,
          },
        ],
      },
    },
  ],
])

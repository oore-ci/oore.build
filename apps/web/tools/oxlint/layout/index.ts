import { defineRule, eslintCompatPlugin } from '@oxlint/plugins'

import type { ESTree } from '@oxlint/plugins'

/** DESIGN.md, "Layout primitives": the most utilities one element may list inline. */
const MAX_UTILITIES = 8

/** Class helpers whose string arguments all land on the same element. */
const CLASS_HELPERS = new Set(['cn', 'clsx', 'cx'])

function countUtilities(value: string): number {
  return value.split(/\s+/).filter(Boolean).length
}

function staticClassText(node: ESTree.Node): string | null {
  // A quoted `raw` marks a string literal; numbers and booleans are unquoted.
  if (node.type === 'Literal' && node.raw !== null && /^['"]/.test(node.raw)) {
    return String(node.value)
  }
  if (node.type === 'TemplateLiteral') {
    return node.quasis.map((quasi) => quasi.value.cooked ?? '').join(' ')
  }
  return null
}

interface ClassCounts {
  /** Utilities that always apply: plain strings, alone or as helper arguments. */
  always: number
  /** The largest single conditional string, such as `active && '...'`. */
  largestConditional: number
}

function collect(node: ESTree.Node, conditional: boolean, counts: ClassCounts) {
  const text = staticClassText(node)
  if (text !== null) {
    const count = countUtilities(text)
    if (conditional) {
      counts.largestConditional = Math.max(counts.largestConditional, count)
    } else {
      counts.always += count
    }
    return
  }
  if (node.type === 'JSXExpressionContainer') {
    if (node.expression.type !== 'JSXEmptyExpression') {
      collect(node.expression, conditional, counts)
    }
    return
  }
  if (
    node.type === 'CallExpression' &&
    node.callee.type === 'Identifier' &&
    CLASS_HELPERS.has(node.callee.name)
  ) {
    for (const argument of node.arguments) {
      collect(argument, conditional, counts)
    }
    return
  }
  if (node.type === 'ArrayExpression') {
    for (const element of node.elements) {
      if (element !== null) collect(element, conditional, counts)
    }
    return
  }
  if (node.type === 'ConditionalExpression') {
    collect(node.consequent, true, counts)
    collect(node.alternate, true, counts)
    return
  }
  if (node.type === 'LogicalExpression') {
    collect(node.right, true, counts)
  }
}

/**
 * Fail a `className` that lists more than eight utilities inline. Recipe
 * calls such as `stackVariants()` do not count; strings passed to `cn()`
 * add up, so splitting a long list across arguments does not pass.
 */
const maxClassUtilitiesRule = defineRule({
  meta: {
    type: 'suggestion',
    docs: {
      description: `Disallow more than ${MAX_UTILITIES} inline utilities on one element.`,
    },
    messages: {
      tooMany:
        '{{count}} inline utilities on one element; the limit is {{max}}. Use a layout primitive or a recipe from lib/recipes.ts (DESIGN.md, "Layout primitives").',
    },
  },
  createOnce(context) {
    return {
      JSXAttribute(node) {
        if (node.name.type !== 'JSXIdentifier') return
        const name = node.name.name
        if (name !== 'className' && !name.endsWith('ClassName')) return
        if (node.value === null) return
        const counts: ClassCounts = { always: 0, largestConditional: 0 }
        collect(node.value, false, counts)
        const count = Math.max(counts.always, counts.largestConditional)
        if (count > MAX_UTILITIES) {
          context.report({
            node,
            messageId: 'tooMany',
            data: { count: String(count), max: String(MAX_UTILITIES) },
          })
        }
      },
    }
  },
})

const layoutPlugin = eslintCompatPlugin({
  meta: { name: 'layout' },
  rules: {
    'max-class-utilities': maxClassUtilitiesRule,
  },
})

export default layoutPlugin

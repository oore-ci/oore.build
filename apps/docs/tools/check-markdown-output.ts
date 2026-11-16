// Build check for the Markdown twins of the docs (D1, #1051): every HTML page
// in dist has `<path>.md`, llms.txt lists every authored page, and
// llms-full.txt carries every authored page and the API operation index.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import path from 'node:path'

const dist = path.resolve(import.meta.dir, '../dist')
const origin = 'https://docs.oore.build'

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = path.join(dir, name)
    return statSync(full).isDirectory() ? walk(full) : [full]
  })
}

const failures: string[] = []
const htmlPages = walk(dist)
  .filter((file) => file.endsWith('.html'))
  .map((file) => path.relative(dist, file))
  .filter((file) => file !== '404.html')

for (const page of htmlPages) {
  const twin = page.replace(/\.html$/, '.md')
  if (!existsSync(path.join(dist, twin))) {
    failures.push(`${page} has no Markdown twin ${twin}`)
  }
}

const llms = readFileSync(path.join(dist, 'llms.txt'), 'utf8')
const full = readFileSync(path.join(dist, 'llms-full.txt'), 'utf8')
const authored = htmlPages.filter((page) => !page.startsWith('openapi/'))
for (const page of authored) {
  const url = `${origin}/${page.replace(/\.html$/, '.md')}`
  const isCategory = page.startsWith('reference/api/categories/')
  if (!llms.includes(`(${url})`)) failures.push(`llms.txt does not list ${url}`)
  if (!isCategory) {
    const source =
      page === 'index.html'
        ? `${origin}/`
        : `${origin}/${page.replace(/\.html$/, '')}`
    if (!full.includes(`Source: ${source}\n`)) {
      failures.push(`llms-full.txt does not include ${source}`)
    }
  }
}
const operations = htmlPages.filter((page) =>
  page.startsWith('openapi/operations/'),
)
for (const page of operations) {
  const url = `${origin}/${page.replace(/\.html$/, '.md')}`
  if (!full.includes(`(${url})`))
    failures.push(`llms-full.txt does not index ${url}`)
}

for (const file of [
  ...htmlPages.map((page) => page.replace(/\.html$/, '.md')),
  'llms.txt',
  'llms-full.txt',
]) {
  const text = readFileSync(path.join(dist, file), 'utf8')
  const outsideFences = text.split(/^```[\s\S]*?^```/m).join('')
  const leftover = /<\/?(?:Callout|Cards?|Steps?|Tabs?)\b/.exec(outsideFences)
  if (leftover) failures.push(`${file} still has MDX markup: ${leftover[0]}`)
}

if (failures.length > 0) {
  console.error(failures.join('\n'))
  console.error(`\n${failures.length} Markdown output problem(s).`)
  process.exit(1)
}
console.log(
  `Markdown output OK: ${htmlPages.length} pages, ${operations.length} API operations, llms.txt and llms-full.txt.`,
)

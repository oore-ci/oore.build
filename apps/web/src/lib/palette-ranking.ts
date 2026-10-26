export interface RankableItem {
  /** What the query is matched against, e.g. the row label and id. */
  value: string
  keywords?: Array<string>
}

export interface RankableSection<TItem extends RankableItem> {
  heading: string
  items: Array<TItem>
}

type Scorer = (
  value: string,
  search: string,
  keywords?: Array<string>,
) => number

/**
 * Filters and orders palette sections for a query: rows that score zero are
 * dropped, rows sort by score inside a section, and sections sort by their
 * best row. A row that contains the query as typed beats a loose fuzzy
 * match, so `run ios` ranks "Run iOS Release" above "Run build…". With no
 * query, the sections keep their authored order.
 */
export function rankPaletteSections<TItem extends RankableItem>(
  sections: ReadonlyArray<RankableSection<TItem>>,
  search: string,
  score: Scorer,
): Array<RankableSection<TItem>> {
  const query = search.trim().toLowerCase()
  if (!query) return sections.filter((section) => section.items.length > 0)

  return sections
    .map((section) => {
      const scored = section.items
        .map((item) => {
          const base = score(item.value, query, item.keywords)
          const exact = base > 0 && item.value.toLowerCase().includes(query)
          return { item, score: exact ? 1 + base : base }
        })
        .filter((entry) => entry.score > 0)
        .sort((left, right) => right.score - left.score)
      return {
        heading: section.heading,
        items: scored.map((entry) => entry.item),
        best: scored[0]?.score ?? 0,
      }
    })
    .filter((section) => section.best > 0)
    .sort((left, right) => right.best - left.best)
    .map(({ heading, items }) => ({ heading, items }))
}

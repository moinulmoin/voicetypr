// Pure share-card helpers split from ShareStatsModal.tsx so the component file
// only exports components.

const WORDS_PER_PAGE = 250;

export function getShareOutcome(totalWords: number): {
  pages: number;
  line: string;
} {
  if (totalWords <= 0) {
    return { pages: 0, line: "Record a thought and start the count" };
  }
  const pages = Math.max(1, Math.round(totalWords / WORDS_PER_PAGE));
  if (totalWords >= 250) {
    return {
      pages,
      line: `${pages.toLocaleString()} pages I didn’t have to type`,
    };
  }
  return { pages, line: "words I didn’t have to type" };
}

export { WORDS_PER_PAGE };

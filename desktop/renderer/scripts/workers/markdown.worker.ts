import { renderMarkdown } from "../features/markdown-core.ts";

type MarkdownJob = { key: string; source: string };

self.addEventListener("message", event => {
  const { requestId, jobs } = event.data as { requestId: number; jobs: MarkdownJob[] };
  const html = Object.fromEntries(jobs.map(job => [job.key, renderMarkdown(job.source)]));
  self.postMessage({ requestId, html });
});

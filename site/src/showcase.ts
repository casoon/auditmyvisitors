import { ansiToHtml } from '@casoon/pages-theme/ansi';
import type { ShowcaseExample } from '@casoon/pages-theme/showcase';

// Real output of auditmyvisitors 0.3.1, captured from the release binary by
// site/captures/capture.sh. Only help output: every report needs a Google login and
// prints the data of a real property, which has no place on a public page.
const files = import.meta.glob<string>('../captures/*.txt', {
  query: '?raw',
  import: 'default',
  eager: true,
});

export function capture(file: string): string {
  const found = files[`../captures/${file}`];
  if (found === undefined) throw new Error(`Unknown capture: ${file}`);
  return found;
}

const catalogue = [
  {
    slug: 'commands',
    title: 'Commands',
    file: 'help.txt',
    command: 'auditmyvisitors --help',
    tags: ['cli', 'help'],
    description:
      'The five command groups. Without a subcommand, auditmyvisitors opens an interactive menu instead.',
  },
  {
    slug: 'reports',
    title: 'Reports',
    file: 'report.txt',
    command: 'auditmyvisitors report --help',
    tags: ['report', 'help'],
    description: 'Twelve reports, each built from GA4 and, where it applies, Search Console data.',
  },
  {
    slug: 'top-pages',
    title: 'Top pages',
    file: 'report-top-pages.txt',
    command: 'auditmyvisitors report top-pages --help',
    tags: ['report', 'pages'],
    description: 'Pages ranked by sessions, clicks or impressions over a chosen number of days.',
  },
  {
    slug: 'queries',
    title: 'Queries',
    file: 'report-queries.txt',
    command: 'auditmyvisitors report queries --help',
    tags: ['report', 'search console'],
    description: 'Query-level search performance, sortable by clicks, impressions, CTR or position.',
  },
  {
    slug: 'compare',
    title: 'Before/after comparison',
    file: 'report-compare.txt',
    command: 'auditmyvisitors report compare --help',
    tags: ['report', 'change date'],
    description:
      'Two windows around a change date such as a deploy, for the whole site or for one URL.',
  },
  {
    slug: 'export-pdf',
    title: 'PDF export',
    file: 'export-pdf.txt',
    command: 'auditmyvisitors export pdf --help',
    tags: ['export', 'pdf'],
    description: 'A full report as PDF, fetched live, written to ./output/ unless a path is given.',
  },
  {
    slug: 'export-csv',
    title: 'CSV export',
    file: 'export-csv.txt',
    command: 'auditmyvisitors export csv --help',
    tags: ['export', 'csv'],
    description: 'One report as CSV, to stdout or a file. Eight reports can be exported this way.',
  },
];

export const examples: ShowcaseExample[] = catalogue.map(({ file, command, ...meta }) => {
  const source = capture(file);
  return {
    ...meta,
    file: `site/captures/${file}`,
    input: { code: `$ ${command}`, lang: 'sh' },
    output: { html: ansiToHtml(source), kind: 'terminal' },
  };
});

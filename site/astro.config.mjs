// @ts-check
import casoonPages from '@casoon/pages-theme';
import { defineConfig } from 'astro/config';

// Project page: https://casoon.github.io/auditmyvisitors/ — `base` is the GitHub Pages path.
export default defineConfig({
  site: 'https://casoon.github.io/auditmyvisitors',
  base: '/auditmyvisitors/',
  integrations: [
    casoonPages({
      name: 'auditmyvisitors',
      description:
        'Local CLI reporting for Google Analytics 4 and Search Console: overviews, page and query analysis, before/after comparisons and PDF export. No backend, read-only access.',
      repo: 'casoon/auditmyvisitors',
      version: '0.3.1',
      license: 'MIT',
      packages: [
        { label: 'Website', href: 'https://auditmyvisitors.casoon.de/' },
        { label: 'GitHub releases', href: 'https://github.com/casoon/auditmyvisitors/releases' },
      ],
      // No CHANGELOG.md yet: the GitHub releases carry no notes to derive one from.
      changelog: false,
      mdxComponents: './src/mdx-components.ts',
      docsGroups: {
        'getting-started': 'Getting started',
        guides: 'Guides',
        reference: 'Reference',
      },
    }),
  ],
});

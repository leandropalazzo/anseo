# OpenGEO Web

This is the Phase 1 dashboard scaffold for OpenGEO. It is a Next.js TypeScript app with Tailwind CSS, App Router, ESLint, Vitest, React Testing Library, Playwright, and axe integration.

## shadcn/ui

shadcn/ui has been initialized for this scaffold. The baseline config lives in `components.json`; no reusable UI components are generated yet because the first story is limited to repository shape and buildability.

Future UI stories can add components from this directory:

```bash
pnpm dlx shadcn@latest add button
```

Keep generated components under the shadcn aliases in `components.json`.

## Getting Started

First, run the development server:

```bash
pnpm dev
```

Open [http://localhost:3000](http://localhost:3000) with your browser to see the result.

You can start editing the page by modifying `app/page.tsx`. The page auto-updates as you edit the file.

## Verification

```bash
pnpm build
pnpm lint
pnpm test
```

## Notes

The dashboard remains read-only in Phase 1. YAML is the source of truth for project configuration.

You can check out [the Next.js GitHub repository](https://github.com/vercel/next.js) - your feedback and contributions are welcome!

## Deploy on Vercel

The easiest way to deploy your Next.js app is to use the [Vercel Platform](https://vercel.com/new?utm_medium=default-template&filter=next.js&utm_source=create-next-app&utm_campaign=create-next-app-readme) from the creators of Next.js.

Check out our [Next.js deployment documentation](https://nextjs.org/docs/app/building-your-application/deploying) for more details.

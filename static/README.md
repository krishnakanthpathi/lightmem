# LightMem website

The landing page and documentation ship as static HTML, with no build step, package install, external fonts, or client rendering dependency. Vercel serves this directory using the root `vercel.json`.

## Shared UI

- `theme.css`: design tokens and shared header, button, badge, code block, copy control, callout, and footer components.
- `site.js`: progressive copy controls, screen-reader announcements, and deployed Vercel analytics. This module is shared and cached between pages.
- `landing.css`: landing page composition.
- `docs.css`: documentation layout and responsive navigation.
- `docs.js`: local search, installer tabs, command disclosures, and section navigation. Search builds a text index on first use and never hides the document. Scroll tracking uses IntersectionObserver.

All documentation and command anchor IDs are in the HTML. Native links, mobile navigation, and command disclosures work without JavaScript. All installation alternatives remain visible if JavaScript is unavailable.

## Preview

From the repository root:

```sh
python3 -m http.server 4173 --directory static
```

Open `http://localhost:4173/` or `http://localhost:4173/docs.html`. Local links adapt to the Python server; deployed documentation uses `/docs`.

## Content changes

Keep the displayed source version aligned with `Cargo.toml` and `python/pyproject.toml`. CLI examples should match `src/main.rs`; integration and scope behavior is described in the root README. Preserve existing section and command IDs when editing to keep incoming links working.

Check both pages at phone and desktop widths. Exercise search, keyboard result navigation, installer tabs, code copying, command expansion, and mobile navigation. Verify that long code and tables scroll within their containers instead of widening the page.

# Native file icons

Exported from the installed @pierre/trees 1.0.0-beta.4 (Apache-2.0, Copyright 2025 Pierre Computer Company), combined with T3's MIT custom icons in apps/web/src/pierre-icons.ts. Only SVG artwork and filename/extension lookup tables are included; no web tree runtime or new dependency.

Local adaptations: standalone 32px SVG documents, explicit light/dark currentColor, and native embedded-asset lookup tables. Regenerate with Node 24: node scripts/vitre/export-file-icons.mjs (prints an apply_patch patch); verify with --check. Rustfmt may be applied to the generated Rust file.

## Upstream notice

This project includes some code derived from
[@headless-tree/core](https://github.com/lukasbach/headless-tree).

The initial version of this project used `headless-tree` as the underlying tree
implementation. We have since written our own core at `packages/path-store`, but
many of the best ideas from `headless-tree` made their way to `path-store` and
`trees`. It's hard to identify exactly which code this is at this point, but
definitely things like the drag and drop implementation and the general list
approach to rendering and I'm sure more. The work that `@lukasbach` has
contributed to this space is greatly appreciated. `<3`

Original license for `headless-tree/core`:

```
MIT License

Copyright (c) 2023 Lukas Bach

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

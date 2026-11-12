# CamCraft documentation site

The source of the CamCraft documentation site (#688), built with
[Zola](https://www.getzola.org/) (a single binary; no Node toolchain).

```sh
cd site
zola serve          # preview at http://127.0.0.1:1111
zola build          # writes public/ (gitignored)
zola check          # build and check every link
```

- `content/` - the pages, as Markdown: `guide/` (user guide) and
  `developers/` (developer section). A page's place in its section is its
  `weight`.
- `templates/` - our own Tera templates; no third-party theme.
- `static/` - the stylesheet, copied as is.

The version shown on the site comes from `version.toml`, which the release
workflow writes from the workspace version before it builds. Without it the
site says it is a local preview. To preview the released look, write one:
`printf 'version = "0.16.0"\n' > version.toml` (gitignored).

`content/app.md` is the page at `app/` when the site has no browser build
(a local preview, or a release whose wasm build failed). On a release the
`wasm` job builds the browser shell (`web/build-app-wasm.sh --release`) and
the `site` job puts that folder at `app/` in its place (#1103);
`web/check-app-page.sh <folder>` checks it, locally on `target/web` too.
The workflow adds `app = true` to `version.toml` only when it placed the
browser build; the front page claims it only then (`app = false`: it says
the browser version of that release is not available).

The site is built and deployed by the `site` and `pages` jobs of
`.github/workflows/release.yml`, on a release tag only. Write content from
CamCraft's own code and docs: the clean-room protocol in `AGENTS.md`
applies here too.

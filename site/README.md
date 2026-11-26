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
- The guide uses sections in job order. Each folder's `_index.md` sets
  its title and weight. Moved pages keep their old URLs through `aliases`
  in the front matter. Internal `@/` links point to the new source paths.
- `templates/` - our own Tera templates; no third-party theme.
- `static/` - the stylesheet, copied as is, and `vendor/` (elasticlunr, the
  MIT-licensed search library, with its licence beside it).
- Search (#1245): `build_search_index` writes `search_index.en.js`, which the
  header search box loads with the vendored elasticlunr. No third-party
  service; the index is built from each page's title, description and the
  first 2000 characters of its body.

Run the site tests from the repository root with Zola on `PATH`:

```sh
python3 -m unittest discover -s site/tests -v
```

They build the site under a URL prefix and check the guide navigation,
legacy redirects, internal links and operation reference pictures.
The `site` job of `.github/workflows/release.yml` runs the same command
with its pinned Zola (`ZOLA=/tmp/zola`) before it builds the site, so a red
site test fails the release's site (#1832). Each Zola test reads the binary
from `ZOLA` and falls back to `zola` on `PATH`; without either it errors.

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

# Glaux documentation sources

This directory holds everything Glaux ships as documentation. It is embedded in the `glaux-docs` crate, so it is versioned with each release and works offline. The files are plain Markdown and RON, so they can be read here or by a website without Glaux.

## Layout

| Path                 | Contents                                                                      |
|----------------------|-------------------------------------------------------------------------------|
| `en/<id>.md`         | English pages. Other languages use their tag as the directory, such as `es/`. |
| `api/<category>.ron` | The script API catalog, one file per category.                                |
| `assets/`            | Images shared by pages.                                                       |
| `ids.lock`           | Every published page id.                                                      |

A page's id is its path below the language directory without `.md`, and its link is `glaux://<id>`. Link between pages with that form, for example `[spr](glaux://api/graphics/spr)`.

## Writing a page

- Start with a level 1 heading, then a paragraph. The heading is the title and the paragraph is the summary shown in hovers and lists.
- Standard GitHub Flavored Markdown is allowed, including tables and images. Give every image alt text, because it is what a viewer shows when it cannot draw the image. Raw HTML is never rendered, so avoid it.
- A page for a function can have a `## Parameters` table whose first column lists the parameter names. They are compared with the catalog.
- Put Luau or Rhai specifics in a subsection headed exactly `Luau` or `Rhai` (level 3 or deeper), or in a fenced block tagged `luau` or `rhai`. Everything else applies to both languages.
- Characters outside the system font show as a placeholder box. Keep text to what the font can draw, with precomposed letters.

## The API catalog

`api/*.ron` lists the names, parameters and return types of the script API. It is maintained by hand. Tests in `glaux-luau` and `glaux-rhai` check that the names exist, that nothing undocumented is installed, and, for Rhai, the arities. A `Callback` entry, such as `boot`, describes a function the script defines and Glaux calls. It must exist in both languages under one name, and the tests check that the runtimes do not install it and that the catalog lists exactly the callbacks they call. Parameter names, types and return types are not verified, so review them against the runtime when an API changes. See the `glaux_docs::catalog` module documentation.

An entry's id is `api/<category>/<name>` and must match its file name. Every entry needs a page with the same id.

## Checking and publishing ids

- `mise run docs-check` validates the pages, links, images, catalog, ids and UI catalogs.
- `mise run docs-update-ids` adds new pages to `ids.lock`. It never removes an id. A published id must keep resolving, so retire one only by editing `ids.lock` in a reviewed change.

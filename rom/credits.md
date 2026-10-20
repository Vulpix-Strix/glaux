# Editorial credits

`credits.ron` holds the credits the `credits` command rolls on its first page:
who made Glaux and who is thanked. It is embedded in the binary, so changing
it needs a rebuild but no code change. The legal attributions are a separate
matter. They come from `apps/glaux/licenses/` and are never written here.

## Format

```ron
(
    sections: [
        (
            title: "Collaborators",
            items: [
                Person(role: "Music", name: "Ada"),
                Message("Thanks to everyone who tested early builds."),
            ],
        ),
    ],
    authors: {
        "Zughy 32": "Zughy",
    },
)
```

- `sections` roll in the order listed, each under its `title`. A section with
  no `items` is left out.
- `Person(role, name)` shows the role and the name joined by a dotted leader.
- `Message(text)` shows one centered line, for a thank-you or a short note.
- Unknown fields are rejected, so a misspelled key shows up in the tests
  instead of silently disappearing. A file that does not parse shows no
  editorial credits and logs the reason.

## Creative resources

The "Creative Resources" section is not written here. It lists the entries of
the license inventory whose category is `art`. The inventory does not record
authors, so `authors` maps an entry's name to the author shown beside it. An
entry without an author is listed by name alone.

Use only real names and messages. There are no placeholders.

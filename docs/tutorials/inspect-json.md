# Tutorial: inspect saved JSON

[Documentation](../README.md) / Tutorials

Learn to inspect structure before selecting values. This exercise uses a saved
example result, no server, no configuration, and no credentials.

You need `ripmcp`, a shell, and `jq` for the final extraction step. ripmcp itself
does not depend on `jq`.

## 1. Save an example

In a scratch directory, create `result.json`:

```sh
cat > result.json <<'JSON'
{
  "resultType": "complete",
  "content": [{"type": "text", "text": "Found two items"}],
  "structuredContent": {
    "items": [
      {"id": 1, "title": "First item"},
      {"id": 2, "title": "Second item"}
    ]
  },
  "isError": false
}
JSON
```

This is sample data, not the output of a tool you have invoked.

## 2. Inspect without displaying scalar values

```sh
ripmcp shape result.json
```

In `shape.fields`, look for `structuredContent`. Its `items` field is an array of
length 2. Each array element has a numeric `id` and a string `title`. Neither title
value appears in the shape report.

The command accepts stdin too:

```sh
ripmcp shape - < result.json
```

## 3. Request a smaller view

```sh
ripmcp shape result.json --depth 2 --width 1
```

Look for `truncated: true` and `omitted` counts. They describe structure excluded
from this view. They do not mean `result.json` was changed or shortened. Increase
the limits when you need to inspect more structure.

## 4. Select only the titles

Now that you know the path and types, read the values from the saved result:

```sh
jq -r '.structuredContent.items[].title' result.json
```

Expected output:

```text
First item
Second item
```

You have used the same saved document for every view. With a real tool result,
this avoids repeating side effects or receiving different data on a second call.

Next: [make a real tool call](first-call.md), or look up
[shape limits and output](../reference/output.md#shape-reports).

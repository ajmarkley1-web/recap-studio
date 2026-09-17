# Contributing

Recap Studio is a remake of the existing legacy project. Work goes into the Rust
backend and the React frontend.

## Before you start

Set the project up and run it once, end to end: ingest a short chapter, analyze
it, narrate it, look at the storyboard. Most of the design only makes sense once
you have watched a page turn into a panel record and then into narration.

```bash
npm install
npm run app
```

## The one rule that is not negotiable

The narrator prompt in `src-tauri/src/narrator.rs` is verbatim. Do not
paraphrase it, reword it, or "improve" it. It is the entire voice of the app,
and the delivery contract next to it is what guarantees every panel is narrated
in order. Changes to either need a reason beyond taste.

## Pull requests

- Keep the commit history readable: clear messages, one concern per commit,
  squashed before review where it helps.
- Reference the issue a change closes, so it closes on merge.
- Run the checks before pushing:

  ```bash
  cd src-tauri && cargo test
  npm run build
  ```

- For UI changes, attach a screenshot. It makes review much faster.

## Issues

When filing a bug, include the steps to reproduce it and the format of the
source you were working with (manga, manhwa or comic), since reading order
drives most of the panel logic. For a feature request, describe the problem
before the solution.

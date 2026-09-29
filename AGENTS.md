this is the lazyspec repo. we're working on the lazyspec tool, dogfooding the tool.

- always run the dev version of lazyspec with `cargo run`, unless there are build errors
- always use the appropriate /lazy and other lazyspec skills
- most of the time you should be running with --json for machine readable output
- when you update the cli interface, make sure you update the readme appropriately
- when you update the engine, make sure you're accounting for changes in the tui, web view and cli
- when you plan changes to the tui, ensure the web view and cli also have those features (and vice verca)

## Skills

These skills can complement AI driven workflows using lazyspec. They guide an AI agent through a structured documentation lifecycle: propose a design, slice it into stories, plan iterations, build, and review.

### Workflow

`plan-work` is the entry point. It inspects existing documents and routes to the right skill.

A typical flow looks like:

```
plan-work → write-rfc → create-story → create-iteration → build → review-iteration
```

`resolve-context` can be called at any point to gather the full document chain (iteration → story → RFC) before starting work.

### Reference

| Skill              | Description                                                                         |
| ------------------ | ----------------------------------------------------------------------------------- |
| `plan-work`        | Detect existing RFCs, Stories, and Iterations to determine the right starting point |
| `write-rfc`        | Create an RFC with design intent, interface sketches, and derived Stories           |
| `create-story`     | Create a Story with given/when/then acceptance criteria linked to an RFC            |
| `create-iteration` | Create an Iteration with task breakdown and test plan against a Story               |
| `build`            | Execute an Iteration's task breakdown, dispatching per-task with review gates       |
| `review-iteration` | Two-stage review: AC compliance first, code quality second                          |
| `resolve-context`  | Gather the full document chain for an agent before it begins work                   |

### Usage

Add the skills directory to your Claude Code settings or copy individual skills into your project's `.claude/skills/` directory. The `plan-work` skill will handle routing from there.

## License

Some skills adapted from [obra/superpowers](https://github.com/obra/superpowers).

MIT License

Copyright (c) 2025 Jesse Vincent

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

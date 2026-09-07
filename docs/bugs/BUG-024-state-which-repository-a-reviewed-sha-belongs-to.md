---
title: State which repository a reviewed sha belongs to
type: bug
status: reported
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: RFC-068
- related-to: RFC-069
---

Found by the end-of-batch review on the RFC-068 governs batch (`cafb665..eb13d8b`).

The batch fixed a concrete instance of this (`pin` stamped HEAD of the docs repo while validation diffed in the governs root, so rename repair was dead in the split-repo layout — corrected in `eb13d8b`). The underlying ambiguity is still unwritten: nothing states which repository a `reviewed` sha belongs to.

In the single-repo layout the two roots coincide and the question never arises. In the split layout RFC-068 §Configuration supports, they differ, and every reader of `reviewed` has to know which one is meant. The bug that was just fixed came from two pieces of code answering that differently.

RFC-069 is about to build staleness judgements on `reviewed`, so it should state the answer before it does. One line in the README on the user-facing meaning, one in RFC-069 on what it compares against.

# Project Title Resolution Design

## Goal

Show the Codex sidebar conversation title for every island task row, including
work performed by nested subagents. Never expose a turn UUID, session UUID,
agent nickname, or internal execution label as a project title.

## Canonical Title

`session_index.jsonl` is the primary title source because it mirrors the
Codex sidebar. The latest `thread_name` for a session ID is its canonical
project title. The existing persisted title state remains a fallback for
installations that do not provide a session index entry.

## Session Ownership

The adapter first reads session metadata for the scanned sessions and resolves
each `parent_thread_id` to its top-level user session. A task event from a
subagent uses that top-level session's canonical title. A direct user session
uses its own canonical title.

## Safe Fallback

If neither source yields a non-empty title, the adapter displays `Codex 任务`.
It never uses task IDs, session IDs, file names, agent nicknames, or task
payload execution labels as a project name.

## Verification

Tests will cover: a direct project task, a nested subagent task, a missing
title fallback, and the regression in which a task ID previously appeared in
the user interface.

# Security Policy

Recap Studio is a desktop app. It stores API keys in the operating system's
per-user config folder for this app, never in a project folder, and it sends
them only to the provider you selected.

## Reporting a vulnerability

Please report security issues privately rather than opening a public issue.
Open a [security advisory](https://github.com/ajmarkley1-web/Recap-engine-/security/advisories/new)
on this repository with:

- a description of the issue and its impact
- the steps to reproduce it
- the app version, from Settings → About

Please give us a chance to ship a fix before disclosing publicly. We will
acknowledge the report, work on a fix, test it, and release it, and we are happy
to credit you unless you would rather stay anonymous.

## Scope

In scope: anything that leaks an API key, reads or writes files outside a
project folder or the app's own config folder, or executes untrusted content
from an ingested source.

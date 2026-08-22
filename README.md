# danie

One-to-one AI tutor in your terminal. It probes what you already know, plans a
prerequisite DAG, teaches **one reasoning step at a time**, and locks every step
in with a quiz — the Alvar learning loop, implemented natively in Rust.

```
┌────────┐    ┌──────────┐    ┌───────┐    ┌──────┐
│ Probe  │ →  │ Plan DAG │ →  │ Teach │ →  │ Quiz │ ↺  (insert prereq on fail)
└────────┘    └──────────┘    └───────┘    └──────┘
                                   ↓ locked nodes feed SM-2 review queue
                              ┌────────┐
                              │ Review │   danie review
                              └────────┘
```

## Install

```sh
cargo install --path danie
```

## Setup

1. Copy `config.example.toml` to your config directory:

   - Windows: `%APPDATA%\danie\config.toml`
   - Linux/macOS: `~/.config/danie/config.toml`

2. Pick a provider:

   - **Anthropic** — export `ANTHROPIC_API_KEY`
   - **Any OpenAI-compatible endpoint** — set `provider = "openai-compat"`;
     works with OpenAI, DeepSeek, LM Studio, or a **free local Ollama**
     (`base_url = "http://localhost:11434/v1"`, no key needed)

3. Verify:

```sh
danie doctor
```

## Usage

| Command | What it does |
|---|---|
| `danie teach <topic>` | Full loop: probe → plan → teach one node at a time |
| `danie probe <topic>` | Diagnostic quiz only; writes your knowledge map |
| `danie review [topic]` | Spaced-repetition (SM-2) review of due nodes |
| `danie map list` | List stored knowledge maps |
| `danie map show <slug>` | Print a stored map |
| `danie doctor` | Check config and provider connectivity |

Keys: `j/k` or arrows navigate, number keys pick quiz answers, `Enter`
confirms, `Esc` quits (sessions save partial progress).

## Data layout (`<store>/.danie/`, default store is `.`)

```
.danie/
  profile.md          learner profile: language, solid ground, goals, preferences
  maps/<slug>.md      knowledge map per goal: strands (known/edge/unknown/blocked) + quiz log
  sessions/<date>-<slug>.md   session summaries: locked, on the edge, next node, notes
  srs.json            SM-2 spaced-repetition queue
```

Everything is plain markdown/JSON — point Obsidian at `.danie/` if you like.

## Workspace crates

| Crate | Purpose |
|---|---|
| [`danie-core`](crates/danie-core) | Domain models, `.danie/` storage, SKILL.md parser, plan DAGs, SM-2 scheduling |
| [`danie-llm`](crates/danie-llm) | Multi-provider LLM abstraction (Anthropic + OpenAI-compatible), retry policy |
| `danie` | ratatui TUI + headless loop engine |

## Design notes

- The loop engine is fully headless: rendering never touches model output.
- Model responses are strict JSON with fence-stripping and exactly one
  corrective retry before failing cleanly.
- Teaching language follows `profile.md`'s `Language:` line (default `es`);
  UI chrome is English.
- Method credit: Eero Alvar's *How I Use AI to Learn Things*; memory design
  inspired by Nous Research's Hermes Agent (episodic/semantic/procedural).

License: MIT.

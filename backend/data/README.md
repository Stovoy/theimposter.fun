# Data Schema

This directory stores seed data for the game. The backend loads these files at startup to provide locations, roles, and question prompts during development.

## `locations.json`
- Array of location objects in the shape:
  ```json
  {
    "id": 1,
    "name": "Location Name",
    "roles": ["Role A", "Role B", "Role C", "Role D", "Role E", "Role F", "Role G"]
  }
  ```
- `id`: integer, unique per location. Order determines display priority when seeding.
- `name`: human friendly label shown to non-imposters.
- `roles`: exactly seven distinct role titles associated with the location.

## `questions.json`
- Array of question objects in the shape:
  ```json
  {
    "id": "q001",
    "text": "Question prompt?",
    "categories": ["category-a", "category-b"]
  }
  ```
- `id`: string identifier (`q###`) to make referencing questions deterministic.
- `text`: the prompt shown to players.
- `categories`: two or more tags describing the prompt. Tags support backend filtering and host rule toggles.

### Tag Conventions
- Use lower-case kebab-case for category names (e.g., `crowd-level`, `auditory`).
- Questions should mix general-purpose tags (`general`, `observation`) and specific tags (`transportation`, `weather`).
- Add new categories freely; avoid duplicates with differing case/spacing.

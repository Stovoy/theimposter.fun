# The Imposter — Game Rules

## 1. Overview
- Social deduction game for 3–8 players played in person while using the app for coordination.
- Each game consists of multiple rounds. In every round, exactly one player is the Imposter.
- Non-imposters know a shared location and hold distinct roles tied to that location; the Imposter knows only that they are the Imposter.

## 2. Setup
- One player creates a lobby in the app, becoming the Host.
- The Host shares the four-character room code so other players can join on their devices.
- Players enter a display name that will appear in the lobby, round summaries, and leaderboards.
- The Host sets optional rule tweaks before the first round:
  - Enable or disable question categories.
  - Adjust the number of locations included in the game (default 10).
  - Modify round timers or other lobby-specific settings when available.
- When the Host starts the game:
  - A random subset of locations (based on the configured count) is selected.
  - Each location provides up to seven unique roles (supporting up to eight total players including the Imposter).

## 3. Round Structure
- At the start of every round:
  - One player is randomly assigned as the Imposter.
  - All other players are randomly assigned one of the remaining roles tied to the chosen location.
  - Non-imposters see the location and their role on demand via the “View Role” button.
  - The Imposter sees only that they are the Imposter.
- The default round view shows turn order, timers, and actions; players must tap dedicated buttons to reveal their role or the location list.
- A random player is chosen to take the first turn. The active player sees a random question selected from the enabled categories in the backend pool (1,000+ questions).
- On their turn the active player verbally asks the displayed question to any opponent. Afterwards they can tap “Next Question” to pass the turn to the next player in clockwise order (as tracked by the app).
- Players may open the in-app list of active locations at any time.

## 4. Making Guesses
- Any player can press the “Guess” button at any time; the control looks identical for imposters and non-imposters to avoid leaking roles.
  - If the player is a non-imposter, they must accuse a specific player of being the Imposter.
  - If the player is the Imposter, they must guess which location is in play.
- The round ends immediately after a guess:
  - **Correct accusation** (non-imposter picks the Imposter): non-imposters win the round.
  - **Incorrect accusation**: the Imposter wins the round instantly.
  - **Correct location guess** (Imposter identifies the location): the Imposter wins the round.
  - **Incorrect location guess**: non-imposters win the round.
- The Host can abort the round or entire game at any time (e.g., to restart or resolve issues).

## 5. Scoring and Progression
- Wins are tracked per player separately for:
  - Rounds won as the Imposter.
  - Rounds won as a non-imposter.
- Tallies appear in the lobby between rounds and on the end-of-game summary screen.
- After resolving a round, the Host can launch the next round or end the game session.

## 6. Conduct Guidelines
- Discussion should stay in character and follow the selected roles when possible.
- Players should not reveal their role or location explicitly unless making a formal guess through the app.
- The app provides mechanical enforcement (turn order, question prompts, guesses), but players self-moderate verbal interaction.

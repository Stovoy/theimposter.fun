import type { PlayerSummary, RoundSummary } from "./api";

const playerMap = (players: PlayerSummary[]) => {
  return new Map(players.map((player) => [player.id, player.name]));
};

export const describeRoundOutcome = (
  summary: RoundSummary | null,
  players: PlayerSummary[],
) => {
  if (!summary) return null;
  const roster = playerMap(players);
  const { outcome, winner } = summary.resolution;

  if ("CrewIdentifiedImposter" in outcome) {
    const info = outcome.CrewIdentifiedImposter;
    const accuser = roster.get(info.accuser) ?? "Crew";
    const impostor = roster.get(info.impostor) ?? "the imposter";
    return `${accuser} exposed ${impostor}. The crew scored a win.`;
  }

  if ("CrewMisdirected" in outcome) {
    const info = outcome.CrewMisdirected;
    const accuser = roster.get(info.accuser) ?? "A crew member";
    const accused = roster.get(info.accused) ?? "someone innocent";
    const impostor = roster.get(info.impostor) ?? "the imposter";
    return `${accuser} accused ${accused} and missed. ${impostor} stole the round.`;
  }

  if ("ImposterIdentifiedLocation" in outcome) {
    const info = outcome.ImposterIdentifiedLocation;
    const impostor = roster.get(info.impostor) ?? "The imposter";
    return `${impostor} guessed the location (${info.location_name}) and won the round.`;
  }

  if ("ImposterFailedLocationGuess" in outcome) {
    const info = outcome.ImposterFailedLocationGuess;
    const impostor = roster.get(info.impostor) ?? "The imposter";
    return `${impostor} guessed the wrong location. The crew held the line at ${info.actual_location_name}.`;
  }

  return winner === "Crew"
    ? "The crew took the round."
    : "The imposter claimed victory.";
};

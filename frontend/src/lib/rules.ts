import type { GameRules } from "./api";

export const defaultRules: GameRules = {
  max_players: 8,
  round_time_seconds: 120,
  allow_repeated_questions: false,
  location_pool_size: 10,
  question_categories: [],
};

export const clampRuleValue = (key: keyof GameRules, value: number) => {
  if (key === "max_players") return Math.min(Math.max(value, 3), 8);
  if (key === "round_time_seconds") return Math.min(Math.max(value, 30), 600);
  if (key === "location_pool_size") return Math.min(Math.max(value, 1), 15);
  return value;
};

export const normalizeCategories = (categories: string[]) =>
  Array.from(new Set(categories.map((category) => category.toLowerCase())));

export const formatCategory = (category: string) =>
  category
    .split("_")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");

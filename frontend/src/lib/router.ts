import { writable } from "svelte/store";

export type RouteName = "landing" | "lobby" | "round" | "scoreboard";
export type ModalName = "role" | "locations" | "guess" | null;

export interface RouteState {
  name: RouteName;
  params: Record<string, string>;
  modal: ModalName;
  path: string;
}

type NavigateOptions = {
  replace?: boolean;
  modal?: ModalName;
};

const fallbackRoute: RouteState = {
  name: "landing",
  params: {},
  modal: null,
  path: "/",
};

const sanitizeCode = (value: string | undefined) =>
  (value ?? "").replace(/[^a-zA-Z0-9]/g, "").slice(0, 4).toUpperCase();

const readModal = (searchParams: URLSearchParams): ModalName => {
  const modal = searchParams.get("modal");
  if (!modal) return null;
  if (modal === "role" || modal === "locations" || modal === "guess") {
    return modal;
  }
  return null;
};

const routeFromParts = (
  pathname: string,
  searchParams: URLSearchParams,
): RouteState => {
  const trimmed = pathname.replace(/\/{2,}/g, "/").replace(/\/+$/, "");
  const segments = trimmed.split("/").filter(Boolean);

  if (segments.length === 0) {
    return { ...fallbackRoute, modal: readModal(searchParams) };
  }

  if (segments[0] === "lobby" && segments[1]) {
    const code = sanitizeCode(segments[1]);
    return {
      name: "lobby",
      params: { code },
      modal: readModal(searchParams),
      path: `/lobby/${code}`,
    };
  }

  if (segments[0] === "round" && segments[1]) {
    const code = sanitizeCode(segments[1]);
    return {
      name: "round",
      params: { code },
      modal: readModal(searchParams),
      path: `/round/${code}`,
    };
  }

  if (segments[0] === "scoreboard" && segments[1]) {
    const code = sanitizeCode(segments[1]);
    return {
      name: "scoreboard",
      params: { code },
      modal: readModal(searchParams),
      path: `/scoreboard/${code}`,
    };
  }

  return { ...fallbackRoute, modal: readModal(searchParams) };
};

const buildPath = (route: RouteState) => {
  const url = new URL(window.location.href);
  url.pathname = route.path;
  if (route.modal) {
    url.searchParams.set("modal", route.modal);
  } else {
    url.searchParams.delete("modal");
  }
  url.hash = "";
  return `${url.pathname}${url.search}${url.hash}` || "/";
};

const stateFor = (
  name: RouteName,
  params: Record<string, string> = {},
  modal: ModalName = null,
): RouteState => {
  const code = sanitizeCode(params.code);
  switch (name) {
    case "landing":
      return { name, params: {}, modal, path: "/" };
    case "lobby":
      if (!code) return { ...fallbackRoute, modal };
      return { name, params: { code }, modal, path: `/lobby/${code}` };
    case "round":
      if (!code) return { ...fallbackRoute, modal };
      return { name, params: { code }, modal, path: `/round/${code}` };
    case "scoreboard":
      if (!code) return { ...fallbackRoute, modal };
      return {
        name,
        params: { code },
        modal,
        path: `/scoreboard/${code}`,
      };
    default:
      return { ...fallbackRoute, modal };
  }
};

const supportsHistory =
  typeof window !== "undefined" && typeof window.history !== "undefined";

export const createRouter = () => {
  const { subscribe, set } = writable<RouteState>(fallbackRoute);

  const readCurrentRoute = (): RouteState => {
    if (!supportsHistory) return fallbackRoute;
    return routeFromParts(
      window.location.pathname,
      new URLSearchParams(window.location.search),
    );
  };

  if (supportsHistory) {
    set(readCurrentRoute());
    window.addEventListener("popstate", () => {
      set(readCurrentRoute());
    });
  }

  const updateHistory = (route: RouteState, replace = false) => {
    if (!supportsHistory) {
      set(route);
      return;
    }
    const target = buildPath(route);
    if (replace) {
      window.history.replaceState({}, "", target);
    } else {
      window.history.pushState({}, "", target);
    }
    set(route);
  };

  const goTo = (name: RouteName, params?: Record<string, string>, options?: NavigateOptions) => {
    const modal = options?.modal ?? null;
    const route = stateFor(name, params ?? {}, modal);
    updateHistory(route, Boolean(options?.replace));
  };

  const replace = (
    name: RouteName,
    params?: Record<string, string>,
    options?: NavigateOptions,
  ) => {
    const modal = options?.modal ?? null;
    const route = stateFor(name, params ?? {}, modal);
    updateHistory(route, true);
  };

  const openModal = (modal: Exclude<ModalName, null>) => {
    const current = supportsHistory ? readCurrentRoute() : fallbackRoute;
    const route = { ...current, modal };
    updateHistory(route, true);
  };

  const closeModal = () => {
    const current = supportsHistory ? readCurrentRoute() : fallbackRoute;
    if (!current.modal) return;
    const route = { ...current, modal: null };
    updateHistory(route, true);
  };

  return {
    subscribe,
    goTo,
    replace,
    openModal,
    closeModal,
    readCurrentRoute,
  };
};

export const router = createRouter();

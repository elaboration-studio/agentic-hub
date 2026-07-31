import type { Route, Scope } from "./shared";

const ROUTES: Route[] = ["manager", "suites", "skills", "statistics", "config"];

export interface ResolvedAppRoute {
  route: Exclude<Route, "suites">;
  managerScope?: Extract<Scope, "suite">;
}

export function resolveAppRoute(hash: string): ResolvedAppRoute {
  const candidate = hash.replace(/^#\/?/, "") as Route;
  if (candidate === "suites") return { route: "manager", managerScope: "suite" };
  return { route: ROUTES.includes(candidate) ? candidate : "manager" };
}

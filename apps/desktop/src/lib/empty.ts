// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * A shared, frozen empty array used as the fallback for not-yet-loaded query data, so that
 * `useMemo` dependencies stay referentially stable between renders.
 */
export const EMPTY: never[] = Object.freeze([]) as unknown as never[];

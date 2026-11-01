// SPDX-License-Identifier: AGPL-3.0-or-later

export interface LibraryStats {
  threads: number;
  messages: number;
  media: number;
}

export interface LibraryInfo {
  root: string;
  stats: LibraryStats;
}

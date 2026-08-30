/// <reference types="@raycast/api">

/* 🚧 🚧 🚧
 * This file is auto-generated from the extension's manifest.
 * Do not modify manually. Instead, update the `package.json` file.
 * 🚧 🚧 🚧 */

/* eslint-disable @typescript-eslint/ban-types */

type ExtensionPreferences = {
  /** Access Token - Your Bitly access token. */
  "accessToken": string,
  /** undefined - Automatically paste the shortened URL after shortening it. */
  "pasteAfterShortening": boolean
}

/** Preferences accessible in all the extension's commands */
declare type Preferences = ExtensionPreferences

declare namespace Preferences {
  /** Preferences accessible in the `index` command */
  export type Index = ExtensionPreferences & {}
  /** Preferences accessible in the `list-links` command */
  export type ListLinks = ExtensionPreferences & {
  /** Group GUID - Your Bitly group GUID */
  "group_guid": string
}
}

declare namespace Arguments {
  /** Arguments passed to the `index` command */
  export type Index = {}
  /** Arguments passed to the `list-links` command */
  export type ListLinks = {}
}


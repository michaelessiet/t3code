# Bezel thinking orbs

- Source: https://github.com/crabtalk/bezel/tree/5cc5f107deb57387d9635d4ee6955a8aa9f69785/crates/agent/src/orbs
- Revision: `5cc5f107deb57387d9635d4ee6955a8aa9f69785`
- License: MIT (LICENSE retained verbatim).
- Upstream credits: gpui-thinking-orbs (FrancoEscob), based on Jakub Antalik’s thinking-orbs.

Only the orb component and its pure geometry engine are included, not Bezel’s GPUI fork or the rest of its widget library. Local adaptations share Vitre’s GPUI pin, resolve Auto ink through gpui-component’s theme, and use the native standard-library clock. Hidden entities cancel their timer immediately even when unmounted; a debug-only timer query supports native lifecycle verification. Local formatting and a Clippy cleanup are also applied. Retain the upstream license and this provenance on updates.

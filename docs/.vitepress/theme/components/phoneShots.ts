// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * The phone screenshots in `public/screens` (780×1328, no system UI) and the colours of their
 * first and last pixel rows, which the phone mockup extends into its status bar and home strip.
 */
export const PHONE_SHOTS = {
  'phone-hosts': {
    top: 'linear-gradient(90deg, rgb(27 23 45), rgb(43 42 81), rgb(20 23 35))',
    bottom: 'rgb(21 20 27)',
  },
  'phone-terminal': {
    top: 'rgb(21 20 27)',
    bottom: 'linear-gradient(90deg, rgb(21 20 27), rgb(31 29 42) 12%)',
  },
} as const;

export type PhoneShot = keyof typeof PHONE_SHOTS;

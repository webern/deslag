---
eleventyNavigation:
  key: 2FA
  title: Setting up Two-factor Authentication
  parent: Security
  order: 10
---

## Why Two-factor Authentication?
While it is important to have a strong password, to gain extra security, e.g. in case your password or device ever gets compromised, it is a good idea to configure Two-factor Authentication for your account.

With Two-factor Authentication, you will be asked for an authentication code generated using your phone in addition to your password when logging into Codeberg.

That way, your account cannot be compromised even if your password does get compromised, as long as your phone stays safe.

## How to set up Two-factor Authentication

### Prerequisites
You will need an authenticator app installed on your phone.

If you don't already have an authenticator app and you're not sure which app to use, have a look at Aegis Authenticator
([F-Droid](https://f-droid.org/de/packages/com.beemdevelopment.aegis/) | [Google Play Store](https://play.google.com/store/apps/details?id=com.beemdevelopment.aegis&hl=en_US)) or Authenticator ([App Store](https://itunes.apple.com/app/authenticator/id766157276)).

### Step 1: Navigate to your user settings
<picture>
  <source srcset="/assets/images/security/user-settings.webp" type="image/webp">
  <img src="/assets/images/security/user-settings.png" alt="User Settings">
</picture>

### Step 2: Navigate to the Security tab and click on the Enroll button
<picture>
  <source srcset="/assets/images/security/2fa/security-settings.webp" type="image/webp">
  <img src="/assets/images/security/2fa/security-settings.png" alt="Security Settings">
</picture>

### Step 3: Scan the QR code and enter the verification code
<picture>
  <source srcset="/assets/images/security/2fa/qr-scan.webp" type="image/webp">
  <img src="/assets/images/security/2fa/qr-scan.jpg" alt="Scanning QR Code">
</picture>

After scanning the QR code with your app, enter the six-digit code displayed in your app into the "Passcode" field of the settings form, then click "Verify".

### Step 4: Store your scratch token in a safe place
If your phone ever breaks, get lost or stolen, you can recover your account using the scratch token.

That token is showed to you right after setting up 2FA:

<picture>
  <source srcset="/assets/images/security/2fa/scratch-token.webp" type="image/webp">
  <img src="/assets/images/security/2fa/scratch-token.png" alt="Scratch token">
</picture>

Please store that token in a safe place.

### Step 5: Done!
That's it - you have now configured 2FA for your account.

From now on, each time you log into Codeberg, you will be asked for an authentication code from your app, adding a layer of security over using only a password.

## Personal access token
If you push via HTTP (see [Clone & Commit via HTTP](/git/clone-commit-via-http)), an extra step will be needed to create a personal access token. This token will replace your normal password (+ authentication code) on Codeberg.

In your profile settings on Codeberg.org, go to the `Applications` tab.
In the section `Manage Access Tokens`, add a `Token Name` and confirm by clicking on the green `Generate Token`.

<picture>
  <source srcset="/assets/images/security/2fa/applications.webp" type="image/webp">
  <img src="/assets/images/security/2fa/applications.png" alt="applications">
</picture>

Make sure you save the generated token in a safe place, because it will not be shown again.

<picture>
  <source srcset="/assets/images/security/2fa/token-created.webp" type="image/webp">
  <img src="/assets/images/security/2fa/token-created.png" alt="token-created">
</picture>

When asked for your password, just supply the token.

You can create as many token as you like: one for each computer, one for each Git client, one for each session... you decide! You can also revoke tokens at any time by pressing the red `Delete` next to the token (see previous screenshot).

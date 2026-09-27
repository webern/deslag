# Image Relay OAuth Callback Worker

This Worker hosts the HTTPS callback required by Image Relay OAuth and bridges
the authorization response into the native macOS app.

Image Relay Developer app callback URI:

```text
https://imagerelay-oauth.amesvt.com/callback
```

The Worker does not store secrets and does not exchange tokens. It preserves the
OAuth `code`, `state`, `error`, and `error_description` query parameters, then
opens:

```text
imagerelay-client://oauth/callback
```

Token exchange stays in the signed native app, where the Image Relay client
secret is stored in Keychain.


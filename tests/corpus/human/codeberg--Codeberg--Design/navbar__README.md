# Codeberg Navbar

This folder contains the new navbar for Codeberg, for easy inclusion with Gitea and other services.
The original idea and design process can be backtraced to [build-deploy-gitea#17](https://codeberg.org/Codeberg/build-deploy-gitea/pulls/17).

## Usage

### Requirements
```html
<link rel="shortcut icon" href="https://codeberg.org/img/favicon.png">
<link rel="stylesheet" href="http://codeberg.org/fomantic/semantic.min.css">
<link rel="stylesheet" href="http://codeberg.org/css/index.css">
<link rel="stylesheet" href="http://codeberg.org/codeberg.css">
<script src="http://codeberg.org/js/jquery.js"></script>
<script src="http://codeberg.org/fomantic/semantic.min.js"></script>
```

**Warning:** due to CORS issues, the libraries above currently can only be included on codeberg.org directly.
For debugging, you can use the [Cors Everywhere](https://addons.mozilla.org/en-US/firefox/addon/cors-everywhere/) Firefox addon.

### Variables
To set the notification count and user, you will need to set the following constants before including the navbar template:

```javascript
const codebergVariables = {
  u: "momar", // username
  n: 12 // notification count
}
```

### Build as HTML
This can be used for Gitea templates or other pages which require a working version without JavaScript:

```
go run . -context codeberg-repositories.json > build/gitea.tmpl
```

<!--
### Include as JavaScript
This is the most flexible approach - you just need to add the following lines to your document:

```html
<nav id="codeberg-navbar"></nav>
<script
  src="https://design.codeberg.org/navbar/embed.js"
  context-src="https://design.codeberg.org/navbar/codeberg-repositories.json"
  ></script>
```
-->

# Next steps

- [ ] translate everything into more languages
- [ ] create scoped CSS file with only the required classes
- [ ] create a whole Fomantic based design toolkit for Codeberg

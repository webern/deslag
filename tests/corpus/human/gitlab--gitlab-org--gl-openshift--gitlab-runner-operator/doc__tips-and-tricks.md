# Login with a RedHat connect key to push an image

Note: `pbpaste` is for Mac, but Linux has alternatives, replace it as you will.

1. Go to the desired project you wish to push an image to, e.g. [https://connect.redhat.com/project/5930041/images/upload-image](https://connect.redhat.com/project/5930041/images/upload-image)
1. Copy the registry key to the clipboard
1. Login with docker

```
pbpaste | docker login -u unused scan.connect.redhat.com --password-stdin
```
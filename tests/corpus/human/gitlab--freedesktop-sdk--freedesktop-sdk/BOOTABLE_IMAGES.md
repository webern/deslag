# Bootable images

If an application built against Freedesktop SDK needs to be
distributed as a bootable image rather than using Flatpak, Snap or
OCI, we provide example of images showing ways to implement different
features.

Those examples are available in `elements/vm`. They all use Dracut as
initramfs generator, and SystemD as init system. Downstream projects
can of course implement their own initramfs or init.

These example images are tested on QEMU.

All the demo images have user `root` and password `root`.

## How to run the images

### Using QEMU

Here we remind you of some useful parameters for QEMU. For more
advance usage, please refer to the (QEMU
documentation](https://www.qemu.org/documentation/).

`qemu-system-<arch>` should be used. Remember to enable `-enable-kvm`
for better performance.

#### CPU and Memory

The default QEMU CPU is no longer capable of running software built with
certain optimisations, on a modern system:

```
-cpu host
```

should be sufficient.

Additionally, the default ram allocation is insufficient for a desktop
system, so:

```
-m 512M
```

should also be specified.

#### Disk

The images provided except for [QEMU+9P](#qemu-9p) are raw images. So they
should be added to the virtual machine with:

```
-drive file=checkout/disk.img,format=raw,media=disk
```

For 9P images, see section [QEMU+9P](#qemu-9p) for explanations.

#### EFI boot

QEMU should come with OVMF. Depending on the distribution or the
version of QEMU, it might have different path or name.

There will be 2 required file, most likely named `OVMF_CODE.fd` and
`OVMF_VARS.fd`.

`OVMF_VARS.fd` will need to be modified, so it is required to be
copied first. `OVMF_CODE.fd` can be used read-only.

Add the following parameters to the command line:

```
-drive if=pflash,format=raw,unit=0,file=/path/to/OVMF_CODE.fd,readonly=on
-drive if=pflash,format=raw,unit=1,file=OVMF_VARS.fd
```

#### Enabling sound

Sound has been tested with HDA emulated hardware. In order to add
support for sound, add the following to the command line:

```
-soundhw hda
```

#### Network

First, it is needed to a network backend. We recommend to use `user` as
it does not require administrator prividlege

```
-netdev user,id=net1
```

Then it is needed to add an emulated device. These images have been
tested with `e1000`. So add create an e1000 device and connect
it to the previously created backend:

```
-device e1000,netdev=net1
```

#### Graphics

For graphics we recommend enabling OpenGL accelaration. With the GTK front end
use:

```
-device virtio-gpu -display gtk,gl=on
```

For the mouse (or other pointer devices), positioning will work better
making the pointer input as a tablet:

```
-usb -device usb-tablet
```

### Using Virtual Machine Manager (aka `virt-manager`)

When creating a virtual machine using Virtual Machine Manager choose
"Import existing disk image" and import the `.img` file. For the
operation system system choose "Generic default".

In the last page of the virtual machine create assistant, check the
checkbox "Customize configuration before install".

In the "Overview", "Hypervisor Details", "Firmware", select the required
boot firmware, either "BIOS" or "UEFI".

In "Video", change the model to "Virtio" and enable "3D acceleration".

The rest of the default configuration should be fine. You can verify
the network uses `e1000` and sound uses `hda`.

## Image types

There are two example of image using different bootloaders. [Legacy
BIOS](#legacy-bios) uses Syslinux, and [EFI](#efi) uses systemd boot.

For virtual machines booting, section [QEMU+9P](#qemu-9p) uses direct kernel
boot and 9p file system.

In section [OSTree update](#ostree-update), we provide live atomic update using
OSTree.

Finally [Desktop](#desktop) provides pieces required for making a desktop image,
that is sound, desktop environment and flatpak application support.

### Legacy BIOS

The `vm/minimal/bios.bst` provides an example image booting with
Syslinux.

```
bst build vm/minimal/bios.bst
bst checkout vm/minimal/bios.bst checkout
```

Then you can use `checkout/disk.img` QEMU.

### EFI

The `vm/minimal/efi.bst` provides an example image booting with
SystemD boot loader on EFI.


```
bst build vm/minimal/efi.bst
bst checkout vm/minimal/efi.bst checkout
```

Then you can use `checkout/disk.img` QEMU with EDKII.

### QEMU + 9p

This method does not use either an image file nor a bootloader. This
can be useful for application using a virtual machine as an
alternative to containers.

`vm/minimal/virt.bst` provides the root file system.

`vm/boot/virt.bst` provides the kernel and initramfs.

The kernel should be passed as parameter to QEMU with `-kernel` and
the initramfs with `-initrd`. Some parameter to the kernel need to be
passed with `-append`. Here are the required kernel paramters.

```
root=virtfs rootflags=trans=virtio,version=9p2000.L rw
```

Because Dracut mounts 9p root named `virtfs`, this has to be used as
mount tagfor the virtfs parameter, e.g. `-virtfs
local,mount_tag=virtfs,path=<path>`.

It is possible to run the demo with `make run-vm`.

### OSTree update

Before building the demo image, some configuration is needed.

Create file `files/vm/ostree-config/fdsdk.gpg` containing the public
key for the repository.

Create `ostree-config.yml` to contain something like:

```
ostree-remote-url: "http://url/to/repo"
ostree-branch: "the-branch-name"
```

`vm/minimal-ostree/repo.bst` provides a single commit OSTree
repository. This repository can be checked out and committed into a
final OSTree repository with history using `ostree commit
--tree=ref=<commit>`.

`utils/update-repo.sh` provide an example of how manage an OSTree
repository.

To build a bootable image, once a repository is available locally
at `ostree-repo`, just track and build `vm/minimal-ostree/image.bst`.

The image contains eos-updater. To update, just run `eos-updater-ctl update`.

All this process is done automatically with:

* `make ostree-server` to start a OSTree server.
* `make update-ostree` to create a new commit.
* `make run-ostree-vm` to run the a virtual machine.

### Desktop

`vm/desktop/efi.bst` provides and EFI boot, all the Freedesktop SDK
libraries as well the following services:

* Pulseaudio
* Flatpak
* Plymouth with EFI bgrt theme, the standard vendor logo + OS logo.
* Weston on Wayland
* XWayland

It boots directly to Weston as user.

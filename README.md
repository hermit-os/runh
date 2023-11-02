# runh

`run` is a CLI tool for spawning and running RustyHermit containers.
To start RustyHermit application within a isolated lightweight virtual machine, a directory with the application and the loader must be created.
In this example, the binaries will be downloaded from a docker registry.

At first, the required tools will be downloaded and installed to run RustyHermit images.

```sh
$ docker export $(docker create ghcr.io/hermit-os/hermit_env:latest) > hermit-env.tar
$ sudo mkdir -p /run/runh/hermit
$ sudo tar -xf hermit-env.tar -C /run/runh/hermit
```

Afterwards, the RustyHermit application will be download and store in a local directory.

```sh
$ docker export $(docker create ghcr.io/hermit-os/rusty_demo:latest) > runh-image.tar
$ mkdir -p ./runh-image/rootfs
$ tar -xf runh-image.tar -C ./runh-image/rootfs
```

In this case, the application an the loader is stored in the subdirectory `runh-image/rootfs`.

```sh
$ ls ./runh-image/rootfs
drwxr-xr-x 1 stefan stefan     128 May 24 12:27 .
drwxr-xr-x 1 stefan stefan      96 May 24 12:27 ..
-rwxr-xr-x 1 stefan stefan 3651080 May 20 13:53 rusty_demo
-rwxr-xr-x 1 stefan stefan 2225868 May 19 22:50 hermit-loader
```

An OCI specification file is required to start the hypervisor within an isolated environment.
The following commands generate a starter bundle for `rusty_demo`, create and start the container:

```sh
$ cd runh-image
$ sudo runh --root /run/runh spec --bundle . --args /hermit/rusty_demo
$ sudo runh --root /run/runh -l debug create  --bundle . runh-container
$ sudo runh --root /run/runh -l debug start runh-container
```

After a successfull test, the container can be deleted with following command:

```sh
$ sudo runh --root /run/runh -l debug delete runh-container
```

## Funding

The development of this project was partially funded by the European Union’s Horizon 2020 research and innovation programme under grant agreement No 957246 - IoT-NGIN.

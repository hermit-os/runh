# runh

`runh` is a CLI tool for spawning and running HermitOS containers.
To start HermitOS application within a isolated lightweight virtual machine, a container registry with the the HermitOS application and its loader.

To create a container image with the httpd example from the [HermitOS](https://github.com/hermit-os/hermit-rs) repository, the following `Dockerfile` is used.
The example assumes, that `httpd` and `hermit-loader-x86_64` is already created and copied to the directory, which contains the `Dockerfile`.

```Dockerfile
FROM ghcr.io/hermit-os/hermit_env:latest
COPY hermit-loader-x86_64 hermit/hermit-loader
COPY httpd hermit/httpd
CMD ["/hermit/httpd"]
```
The base image `hermit_env` includes an hypervisor, which is able to boot _Hermit OS_ applications.
The image based on _Ubuntu_.
To reduce the image size, the base image _hermit_env_alpine_ can be used.
This image based on [Alpine Linux](https://www.alpinelinux.org), which is a security-oriented, lightweight Linux distribution.
Afterwards, the container image can be created and pushed to the registry.
The registery tag has to replace with the enduser registry.

```sh
$ docker buildx build --tag ghcr.io/hermit-os/httpd:latest --push .
```

## Funding

The development of this project was partially funded by the European Union’s Horizon 2020 research and innovation programme under grant agreement No 957246 - IoT-NGIN.

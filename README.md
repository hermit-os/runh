# runh

`run`is a CLI tool for spawning and running RustyHermit containers.
To start RustyHermit application within a isolated lightweight virtual machine, a directory with the application and the loader must be created.
In this example, the binaries will be downloaded from a docker registry.

```sh
$ mkdir data
$ runh pull -b data -u USERNAME -p PASSWORD  registry.git.rwth-aachen.de/acs/public/hermitcore/rusty-hermit/demo
```

Please note, that is option is only possible, if you have a valid account at the docker registry of the RWTH Aachen University.

In this case, the application an the loader is stored in the subdirectory `data`.

```sh
$ ls data
hermit
$ ls -la data/hermit
drwxr-xr-x 1 stefan stefan     128 May 24 12:27 .
drwxr-xr-x 1 stefan stefan      96 May 24 12:27 ..
-rwxr-xr-x 1 stefan stefan 3651080 May 20 13:53 rusty_demo
-rwxr-xr-x 1 stefan stefan 2225868 May 19 22:50 rusty-loader
```

An OCI specification file is required to start the hypervisor within an isolated environment.
The command `spec` generate a starter file.
Editing of this file is required to achieve desired results.
To run `rusty_demo`, it is required to set the args parameter in the spec to call `rusty_demo`.
This can be done using the sed command or a text editor.
The following commands create a bundle for `rusty_demo`, change the
default args parameter in the spec from `sh` to `/hermit/rusty_demo`:

```sh
$ runh spec -b data
$ sed -i 's;"sh";"/hermit/rusty_demo";' data/config.json
```

## Funding

The development of this project was partially funded by the European Union’s Horizon 2020 research and innovation programme under grant agreement No 957246 - IoT-NGIN.

# Internal Setup

The main setup instructions cover setup of SIMICS using publicly-available SIMICS
distribution. This guide covers setup of SIMICS using Intel's private SIMICS
package manager and packages, to allow more up-to-date package installation and
installation of packages for current and future platforms.

## (Optional) Install Simics GUI Dependencies

This step is optional, if you want to use the Simics GUI to install it, you will need
these dependencies.

For Ubuntu or Debian, install them with:

```sh
sudo apt-get install libatk1.0-0 libatk-bridge2.0-0 libcups2 libgtk-3-0 libgbm1 \
    libasound2
```

On Red Hat or Fedora, install them with:

```sh
sudo dnf install atk cups gtk3 mesa-libgbm alsa-lib
```

## Install SIMICS Dependencies

Internal SIMICS utilizes Kerberos for authentication and SMB to download SIMICS
packages. We recommend using the
[intelize](https://github.com/intel-innersource/applications.provisioning.linux-at-intel.intelize/)
script to set this up automatically, but for temporary machines or VMs a lighter weight
solution may be desired.

First, install Kerberos and SMB Client for your distribution.

Ubuntu:

```sh
sudo apt-get install -y krb5-user smbclient
```

If installing interactively, you will be prompted for your *default realm* during
installation via a prompt like the one below.

```text
Configuring Kerberos Authentication
-----------------------------------

When users attempt to use Kerberos and specify a principal or user name without
specifying what administrative Kerberos realm that principal belongs to, the system
appends the default realm.  The default realm may also be used as the realm of a
Kerberos service running on the local machine.  Often, the default realm is the
uppercase version of the local DNS domain.

Default Kerberos version 5 realm:
```

At the prompt, enter your authentication domain. For North America Intel users, this is
`AMR.CORP.INTEL.COM`. You can find your domain in Workday if you don't know it.

If installing non-interactively, set the contents of `/etc/krb5.conf` to the text below,
replacing the realm with yours if it is different.

```text
[libdefaults]
default_realm = AMR.CORP.INTEL.COM

[realms]

[domain_realm]
```

## Download Simics

If you need internal Simics packages, you will need to follow the internal Simics
download and setup processes. In this case, you likely know what packages you need, and
you can obtain the Simics download [here](https://goto.intel.com/simics). If you don't
know what packages you need, or want to run the examples/tutorials only, download the
SIMICS package manager `ispm-internal-latest-linux64.tar.gz` from
[here](https://goto.intel.com/simics).

## Check kinit

When using `ispm` internally, you need to have Kerberos set up for authentication.
Typically this will be done automatically, for example when installing your OS and
configuring it with
[intelize](https://github.com/intel-innersource/applications.provisioning.linux-at-intel.intelize/).

Before running `ispm`, you'll want to initialize Kerberos by running:

```sh
$ kinit
Password for YOU@XXX.XXXX.XXXXX.COM:
```

You can check that you have a valid ticket by running:

```sh
$ klist
Ticket cache: FILE:/tmp/krb5cc_1000
Default principal: YOU@XXX.XXXX.XXXXX.COM

Valid starting       Expires              Service principal
07/20/2023 11:20:34  07/20/2023 21:20:34  krbtgt/XXX.XXXX.XXXXX.COM@XXX.XXXX.XXXXX.COM
        renew until 07/27/2023 11:20:30
```

Kerberos should be working in this case, but please report issues.

## Install Simics

TSFFS can be built against SIMICS installed either with a CLI or GUI installation of
SIMICS. If you are more comfortable with or used to using the GUI, skip to
[the next section](#install-simics-internal-gui).

Internal Simics installations require you to know which packages you need. If you need
internal packages at all, you likely know the full list. Assuming you downloaded
`ispm-internal-latest-linux64.tar.gz`, you can extract it and install the packages
required to run the samples (Simics-Base and QSP-x86) with the command below.

```sh
mkdir -p "${HOME}/install/simics/ispm"
curl -o "${HOME}/Downlods/ispm-internal-latest-linux64.tar.gz" \
    "https://af01p-sc.devtools.intel.com/artifactory/simics-repos/pub/simics-installer/intel-internal/ispm-internal-latest-linux64.tar.gz"
tar -C "${HOME}/install/simics/ispm" --strip-components=1 \
    -xzvf "${HOME}/Downloads/ispm-internal-latest-linux64.tar.gz"
"${HOME}/install/simics/ispm/ispm" \
    install \
    --install-dir "${HOME}/install/simics" \
    --package-repo "https://af01p-sc.devtools.intel.com/artifactory/simics-repos/pub/simics-6/linux64/" \
    1000-6.0.172 \
    2096-6.0.69
```

## Install SIMICS

If you already installed using the ISPM CLI, skip this step and move on to the
[next](#set-up-simics_home-internal).

After downloading the `ispm-internal-latest-linux64.tar.gz` tarball, create a directory
to install your packages into, and extract ISPM into it, then run the ISPM GUI.

```sh
mkdir -p "${HOME}/install/simics/ispm"
curl -o "${HOME}/Downlods/ispm-internal-latest-linux64.tar.gz" \
    "https://af01p-sc.devtools.intel.com/artifactory/simics-repos/pub/simics-installer/intel-internal/ispm-internal-latest-linux64.tar.gz"
tar -C "${HOME}/install/simics" --strip-components=1 \
    -xzvf "${HOME}/Downloads/ispm-internal-latest-linux64.tar.gz"
"${HOME}/install/simics/ispm/ispm-gui"
```

The GUI will open and prompt you to select a directory to install packages into. Select
the directory you just created (`${HOME}/install/simics` in this case).

![Select install directory](../docs/images/SETUP_Select_Install_Directory.png)

After selecting your install directory, type `qsp` into the search box. This will bring
up a multi-package install with SIMICS Base, QSP-x86, and a few additional pacakges.

In the bottom right corner, select `Install Only` and select the option.

![Install Only](../docs/images/SETUP_Install_Only.png)

On the next screen, select `Proceed`.

![Proceed](../docs/images/SETUP_Install_Proceed.png)

After the installation completes, you'll see all green bars:

![Install complete](../docs/images/SETUP_Install_Finished.png)

Finally, check that all the packages installed:

```sh
$ ls -lh "${HOME}/install/simics"
total 126M
drwxr-xr-x. 1 rhart rhart  752 Dec 31  1969 ispm
-rw-r--r--. 1 rhart rhart 126M Jun  9 10:44 ispm-internal-latest-linux64.tar.gz
drwxr-xr-x. 1 rhart rhart   86 Jul 19 17:00 manifests
drwxr-xr-x. 1 rhart rhart  312 Jul 19 16:59 simics-6.0.172
drwxr-xr-x. 1 rhart rhart  156 Jul 19 16:59 simics-crypto-engine-6.0.2
drwxr-xr-x. 1 rhart rhart  148 Jul 19 16:59 simics-eclipse-6.0.33
drwxr-xr-x. 1 rhart rhart  152 Jul 19 16:59 simics-gdb-6.0.0
drwx------. 1 rhart rhart    0 Jul 19 17:00 simics-pkg-mgr-tmp-YOUR_USERNAME
drwxr-xr-x. 1 rhart rhart  172 Jul 19 17:00 simics-qsp-clear-linux-6.0.14
drwxr-xr-x. 1 rhart rhart  140 Jul 19 16:59 simics-qsp-cpu-6.0.17
drwxr-xr-x. 1 rhart rhart  168 Jul 19 16:59 simics-qsp-x86-6.0.70
```

## Set up SIMICS_HOME

In the root of this project, create a file `.env` containing a line like the below that
points to your `SIMICS_HOME` directory (the `--install-dir` argument you passed to
`ispm` in the last step, replace `YOUR_USERNAME` with your username).

```sh
SIMICS_HOME=/home/YOUR_USERNAME/install/simics/
```
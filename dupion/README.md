# dupion

WIP Tool for finding duplicate files and folders


## Features

Implemented:
- find/scan for duplicate files and folders
- search in archives (libarchive)
- HDD opimized sequential read/scan
- cache (rudimentary)  

WIP:
- deduplication features (e.g. btrfs/dedup_range, interactive removal)

TBD:
- nested archive search
- improved cache (e.g. DB-based)

## Usage (reduced)

```
dupion 0.2.0
Find duplicate files and folders

USAGE:
    dupion [FLAGS] [OPTIONS] [dirs]...

FLAGS:
    -h, --help             Prints help information
        --no-cache         don't read or write cache file
    -a, --read-archives    also search inside archives. requires to scan and hash every archive
    -V, --version          Prints version information
    -v, --verbose          spam stderr

OPTIONS:
        --archive-cache-mem <archive-cache-mem>    threaded archive read cache limit in MiB [default: 1024.0]
    -s, --shadow-rule <shadow-rule>
            show shadowed files/directory (shadowed are e.g. childs of duplicate folders) (0-3)
            0: show ALL, including pure shadowed groups
            1: show all except pure shadowed groups
            2: show shadowed only if there is also one non-shadowed in the group
            3: never show shadowed
             [default: 2]
    -t, --threads <threads>
            number of threads for zip decoding, 0 = RAYON_NUM_THREADS or num_cpu logical count [default: 0]


ARGS:
    <dirs>...    directories to scan. cwd if none given
```

## Examples

```
dupion -s 1 >found_dups # find duplicates in current dir and print dup groups into found_dups, also show more shadowed dups
dupion -a dir_a dir_b >found_dups # find duplicates dir_a and dir_b, also search in archives
```

## Technology

- In first pass files are recursively discovered and metadata (size, mtime, ...) are queried.
- In second pass it will hash all files with non-unique size.
- Duplicates are matched per HashMap-based Size/Hash groups.
- Directory hash is calculated by filename and hash of the child entries

## Install/Update

```
git clone -b stable https://github.com/qwertz19281/dupion
cd dupion
git pull
cargo update
cargo install -f --path . dupion
```
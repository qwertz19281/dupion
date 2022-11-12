# dupion

Tool for finding duplicate files and folders, even inside archives, listing results in different ways or to deduplicate.

## Features

Implemented:
- Find/Scan for duplicate files and folders
- Search in archives (libarchive)
- HDD optimized sequential read/scan/dedup  
- btrfs/ioctl_file_dedupe_range deduplication mode

TODO:
- Nested archive search
- Improved cache (e.g. DB-based)
- More deduplication features

## Install / Update

stable branch
```
cargo install --git https://github.com/qwertz19281/dupion --branch stable 
```
master branch
```
cargo install --git https://github.com/qwertz19281/dupion
```

## Examples

Find duplicates in current dir and print dup groups, also show more shadowed dups
```
dupion -s 1
```
Find duplicates dir_a and dir_b, also search in archives
```
dupion -a dir_a dir_b >found_dups
```
Deduplicate, no dups listing, don't use cache file
```
dupion --dedup btrfs --no-cache -o - /home/user
```

## Usage (reduced)

```
Usage: dupion [OPTIONS] [DIRS]...

Arguments:
  [DIRS]...
          Directories to scan. cwd if none defined

Options:
  -o, --output <OUTPUT>
          Results output mode (g/t/d/-), what type of result should be printed
          groups: duplicate entries in sorted size groups
          tree: json as tree
          diff: like tree, but exact dir comparision, reveals diffs and supersets
          -: disabled
          
          [default: g]
          [possible values: groups, tree, diff, disabled]

  -s, --shadow-rule <SHADOW_RULE>
          Set how files/directory should be hidden/omitted (shadowed are e.g. childs of duplicate dirs) (0-3)
          0: show ALL, including pure shadowed groups
          1: show all except pure shadowed groups
          2: show shadowed only if there is also one non-shadowed in the group
          3: never show shadowed
          
          [default: 2]

  -a, --read-archives
          Also search inside archives. requires to scan and hash every archive

      --dedup <DEDUP>
          Deduplication mode (-/btrfs). Disabled by default
          
          btrfs: Use ioctl_file_dedupe_range on supported filesystems
          
          [possible values: btrfs]

      --no-cache
          Don't read or write cache file
```

## Technology

- In first pass files are recursively discovered and metadata (size, mtime, ...) are queried.
- In second pass it will hash all files with non-unique size.
- Duplicates are matched per HashMap-based Size/Hash groups.
- Directory hash is calculated by filename and hash of the child entries
- If deduplicating, the files will be grouped and batch optimized to the available RAM/cache, and then prefetched before issuing deduplication.

## License

dupion (`dupion` and root files) is dual-licensed under either:

* Apache License, Version 2.0 ([LICENSE-APACHE](dupion/LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))
* MIT License ([LICENSE-MIT](dupion/LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))

The third-party crates included in this repository (`libarchive-rust`, `platter-walk`, `reapfrog`, `rust-btrfs`) are used and available under their respective licenses.

Crate           | License
--------------- | ----------------------------------------------------------------
dupion          | [MIT](dupion/LICENSE-MIT) OR [Apache-2.0](dupion/LICENSE-APACHE)
libarchive-rust | [Apache-2.0](libarchive-rust/LICENSE)
platter-walk    | [MPL-2.0](platter-walk/LICENCE)
reapfrog        | [MPL-2.0](reapfrog/LICENCE)
rust-btrfs      | [MIT](rust-btrfs/LICENSE)

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be licensed as above, without any additional terms or
conditions.

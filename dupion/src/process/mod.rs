use super::*;
use state::State;
use group::HashGroup;
use size_format::SizeFormatterBinary;
use opts::Opts;
use vfs::{VfsId, entry::VfsEntryType};
use util::{Hash, Size};
use sha2::{Digest, Sha512};
use std::{sync::Arc, io::Write, cmp::Reverse};

pub mod treestat;
pub mod treediff;

pub fn export(b: &mut State) -> Vec<HashGroup> {
    let tree = &mut b.tree;
    for e in b.hashes.values_mut() {
        e.entries.sort_by_key(|(t,id)| 
            (t.order(),tree[*id].path.clone())
        );
    }

    let mut v = b.hashes.values()
        .cloned()
        .collect::<Vec<_>>();

    v.sort_by_key(|e| {
        let (fd_order,name) = 
            e.entries.get(0).map_or(
                (0,b.tree.static_empty_arc_path.clone()),
                |(typ,id)| (typ.order(),b.tree[*id].path.clone())
            );
        
        (Reverse(e.size),fd_order,name)
    } );
    v
}

pub fn printion(v: &[HashGroup], b: &State, opts: &Opts) {
    for h in v {
        let mut non_shadowed = 0usize;
        let mut shadowed = 0usize;

        let entries = &h.entries.iter()
            .filter(|(t,e)| b.tree[*e].is2(*t) )
            .collect::<Vec<_>>();

        if entries.len() <= 1 {continue;}

        for (t,e) in entries.iter() {
            let e = &b.tree[*e];
            if e.exists() {
                if e.shadowed(*t) {
                    shadowed += 1;
                }else{
                    non_shadowed += 1;
                }
            }
        }
        
        //assert!(shadowed != 1);

        let hide_shadowed = {
            match opts.shadow_rule {
                0 => false,
                1 => non_shadowed == 0,
                2 => non_shadowed != 1,
                3 => true,
                _ => unreachable!(),
            }
        };

        if hide_shadowed && non_shadowed <= 1 {continue;}

        println!("\nGroup {}B", SizeFormatterBinary::new(h.size));
        for (t,e) in entries {
            let e = &b.tree[*e];

            if !hide_shadowed || !e.shadowed(*t) {
                assert_eq!(e.size(*t).unwrap(),h.size);
                let tt = t.icon2(e.is_dir);
                println!(
                    "   {}{} {}",
                    tt,
                    if e.shadowed(*t) {'S'} else {' '},
                    opts.path_disp(&e.path)
                );
            }
        }
    }
}

pub fn calculate_dir_hash(state: &mut State, id: VfsId) -> Result<(Size,Hash),()> {
    assert!(state.tree[id].dir_hash.is_none());
    assert!(state.tree[id].dir_size.is_none());
    if state.tree[id].is_file && !state.tree[id].is_dir {
        let (size,hash) = state.tree[id].file_props();
        //eprintln!("{},{:?},{}",state.tree[id].path.to_string_lossy(),state.tree[id].size,state.tree[id].hash.is_some());
        assert!(size.is_some());
        return Ok((
            size.ok_or(())?,
            hash.ok_or(())?,
        ));
    }
    if !state.tree[id].is_dir {
        return Err(());
    }
    //eprintln!("Hash Dir {}",state.tree[id].path.to_string_lossy());
    assert!(state.tree[id].is_dir);
    //assert!(sf_size.is_none()); //TODO invalid if archive support

    let mut size = 0;
    let mut hashes = Vec::new();

    let calced = state.tree[id].childs.clone();

    let calced: Vec<_> = calced.iter()
        .filter(|&&c| state.tree[c].exists() )
        .collect();

    let calced: Vec<_> = calced.iter()
        .map(|&&c| (c,calculate_dir_hash(state, c)) )
        .collect();

    for (c,r) in calced {
        let (s,h) = r?;
        size += s;
        hashes.push((
            h,
            state.tree[c].path.file_name()
                .map(|s| s.to_str().unwrap() )
                .unwrap_or(""),
        ));
    }

    hashes.sort();

    let mut hasher = Sha512::new();

    for (h,n) in hashes {
        hasher.write(n.as_ref()).unwrap();
        hasher.write(&**h).unwrap();
    }

    let hash = Arc::new(hasher.finalize());

    //eprintln!("Hashed Dir {}",state.tree[id].path.to_string_lossy());
    //eprintln!("{} {}",size,encode_hash(&hash));

    state.tree[id].dir_size = Some(size);
    state.tree[id].dir_hash = Some(hash.clone());
    state.tree[id].valid = true;

    state.push_to_size_group(id,false,true).unwrap();
    state.push_to_hash_group(id,false,true).unwrap();

    if state.tree[id].is_file {
        let (size,hash) = state.tree[id].file_props();
        assert!(size.is_some());
        return Ok((
            size.unwrap(),
            hash.unwrap(),
        ));
    }

    Ok((size,hash))
}

pub fn find_shadowed(state: &mut State, id: VfsId) {
    if !state.tree[id].exists() {return;}

    if let Some(hash) = state.tree[id].file_hash.clone() {
        if state.more_than_one_hash(&hash) {
            state.tree[id].dir_shadowed = true;
            state.tree.for_recursive(id, false, |e| {
                e.dir_shadowed = true;
                e.file_shadowed = true;
            });
            return;
        }
    }
    if let Some(hash) = state.tree[id].dir_hash.clone() {
        if state.more_than_one_hash(&hash) {
            state.tree.for_recursive(id, false, |e| {
                e.dir_shadowed = true;
                e.file_shadowed = true;
            });
            return;
        }
    }
    for c in state.tree[id].childs.clone() {
        assert!(c != id);
        find_shadowed(state,c);
    }
}

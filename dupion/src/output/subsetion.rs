use std::cmp::Reverse;
use std::collections::hash_map::Entry;

use rustc_hash::FxHashMap;

use crate::opts::Opts;
use crate::state::State;
use crate::vfs::{Vfs, VfsId};

pub fn print_subsetion(state: &mut State, opts: &Opts) {
    sort_em(&mut state.tree);

    let mut matches = SMatches(FxHashMap::with_capacity_and_hasher(state.tree.entries.len(),Default::default()));

    let candis: Vec<VfsId> = state.tree.entries.iter()
        .enumerate()
        .filter(|(_,e)| e.is_dir )
        .filter(|(_,e)| e.childs.len() != 0 )
        .filter(|(_,e)| e.dir_size.unwrap_or(0) != 0 )
        .map(|(i,_)| VfsId{evil_inner:i} )
        .collect();

    all_to_all(&candis, |a,b| {comp_dirs(*a,*b,&mut matches,state);} );

    drop(candis);

    let mut matches: Vec<SMatch> = matches.0.values()
        .filter(|s| s.is_shadowed() )
        .cloned()
        .collect();

    matches.sort_by_key(|s| Reverse(match s {
        SMatch::Eq(a,_,_) => state.tree[*a].dir_size.unwrap_or(0),
        SMatch::Super(a,b,_) => 
            state.tree[*a].dir_size.unwrap_or(0).min( state.tree[*b].dir_size.unwrap_or(0) ),
    }));

    for m in matches {
        match m {
            SMatch::Eq(a,b,_) => eprintln!(
                "\t{} == {}",
                opts.path_disp(&*state.tree[a].path),
                opts.path_disp(&*state.tree[b].path),
            ),
            SMatch::Super(a,b,_) => eprintln!(
                "\t{} >> {}",
                opts.path_disp(&*state.tree[a].path),
                opts.path_disp(&*state.tree[b].path),
            ),
        }
    }
}

pub fn comp_dirs(a: VfsId, b: VfsId, matches: &mut SMatches, state: &mut State) -> Ordr {
    if let Some(m) = matches.get(a,b) {
        match m {
            SMatch::Eq(aa,bb,_) => {
                if !((aa==a&&bb==b) || (aa==b&&bb==a)) {
                    panic!();
                }
                return Ordr::Eq;
            }
            SMatch::Super(aa,bb,_) => {
                if aa==a && bb==b {
                    return Ordr::Super;
                } else if aa==b && bb==a {
                    return Ordr::Sub;
                } else {
                    panic!();
                }
            }
        }
    }

    let newer_mode = false;

    let max_len = state.tree[a].childs.len().max( state.tree[b].childs.len() );

    let mut shadow_candidates: Vec<(VfsId,VfsId)> = Vec::with_capacity(max_len);

    let a_iter = state.tree[a].childs.clone();
    let b_iter = state.tree[b].childs.clone();

    let cmp_fn = |a,b| -> Ordr {
        let aa = &state.tree[a];
        let bb = &state.tree[b];

        let aname = aa.path.file_name();
        let bname = bb.path.file_name();

        if let (Some(a),Some(b)) = (aname,bname) {
            if a != b {
                return Ordr::Nope;
            }
        } else {
            return Ordr::Nope;
        }

        if newer_mode {
            todo!()
        } else {
            if let (Some(al),Some(ah),Some(bl),Some(bh)) = (aa.file_size,aa.file_hash.as_ref(),bb.file_size,bb.file_hash.as_ref()) {
                if al == bl && ah == bh {
                    return Ordr::Eq;
                }
            }
            if let (Some(0),Some(0)) = (aa.file_size,bb.file_size) {
                return Ordr::Eq;
            }
            if let (Some(al),Some(ah),Some(bl),Some(bh)) = (aa.dir_size,aa.dir_hash.as_ref(),bb.dir_size,bb.dir_hash.as_ref()) {
                if al == bl && ah == bh {
                    return Ordr::Eq;
                }
            }
            if let (Some(0),Some(0)) = (aa.dir_size,bb.dir_size) {
                return Ordr::Eq;
            }
            match (aa.is_dir,bb.is_dir) {
                (true,true) => {
                    shadow_candidates.push((a,b));
                    comp_dirs(a,b,matches,state)
                },
                _ => Ordr::Nope,
            }
        }
    };

    let o = relationion(
        a_iter.into_iter(),
        b_iter.into_iter(),
        cmp_fn,
    );

    match o {
        Ordr::Eq => matches.set_eq(a, b),
        Ordr::Super => matches.set_super(a, b),
        Ordr::Sub => matches.set_super(b, a),
        Ordr::Nope => {},
    }

    for (a,b) in shadow_candidates {
        match o {
            Ordr::Eq => matches.set_eq_shadowed(a, b),
            Ordr::Super => matches.set_super_shadowed(a, b),
            Ordr::Sub => matches.set_super_shadowed(b, a),
            Ordr::Nope => {},
        }
    }

    o
}

// find super dirs containing anything contained by subdir and more
// - superdir always contains more or equal files as subdir

pub fn relationion<T: Copy>(
    mut a: impl ExactSizeIterator<Item=T>,
    mut b: impl ExactSizeIterator<Item=T>,
    mut cmp: impl FnMut(T,T) -> Ordr,
) -> Ordr {
    let mut a_pot = false;
    let mut b_pot = false;
    let mut eq_len = false;

    if a.len() == b.len() {
        eq_len = true;
    } else if a.len() > b.len() {
        a_pot = true;
    } else {
        b_pot = true;
    }

    let mut a_cur = a.next();
    let mut b_cur = b.next();

    loop {
        assert!(!(a_pot&&b_pot));
        // if a_pot and b_pot false, eq_len
        if let (Some(aa),Some(bb)) = (a_cur,b_cur) {
            match cmp(aa,bb) {
                Ordr::Eq => {
                    a_cur = a.next(); b_cur = b.next();
                },
                Ordr::Super => {
                    if b_pot {
                        return Ordr::Nope;
                    }
                    a_pot = true;
                    a_cur = a.next(); b_cur = b.next();
                },
                Ordr::Sub => {
                    if a_pot {
                        return Ordr::Nope;
                    }
                    b_pot = true;
                    a_cur = a.next(); b_cur = b.next();
                },
                Ordr::Nope => {
                    if a_pot {
                        a_cur = a.next();
                    } else if b_pot {
                        b_cur = b.next();
                    } else {
                        return Ordr::Nope;
                    }
                },
            }
        } else if let (Some(aa),None) = (a_cur,b_cur) {
            // only a_pot, else fail with no match, take a
            /*match (a_pot,b_pot) {
                (false,false) => return Ordr::Eq, //a_pot = true; a=next
                (true,false) => return Ordr::Super, //a=next
                (false,true) => return Ordr::Sub, //return Nope
                _ => unreachable!(),
            }*/
            if b_pot {
                return Ordr::Nope;
            }
            a_pot = true;
            a_cur = a.next();
        } else if let (None,Some(bb)) = (a_cur,b_cur) {
            // only b_pot, else fail with no match, take b
            if a_pot {
                return Ordr::Nope;
            }
            b_pot = true;
            b_cur = a.next();
        } else {
            // don't assert eq_len
            match (a_pot,b_pot) {
                (false,false) => return Ordr::Eq,
                (true,false) => return Ordr::Super,
                (false,true) => return Ordr::Sub,
                _ => unreachable!(),
            }
        }
    }

    unreachable!()
}

pub struct SMatches(pub FxHashMap<(usize,usize),SMatch>);

impl SMatches {
    pub fn get(&self, a: VfsId, b: VfsId) -> Option<SMatch> {
        self.0.get(&Self::key(a,b)).cloned()
    }
    
    pub fn set_super(&mut self, sup: VfsId, sub: VfsId) {
        let e = self.0.entry(Self::key(sup,sub));
        match e {
            Entry::Vacant(e) => {
                e.insert(SMatch::Super(sup,sub,false));
            }
            Entry::Occupied(e) => {
                if let SMatch::Super(a,b,_) = e.get() {
                    if !(*a == sup && *b == sub) {
                        panic!();
                    }
                }
            }
        }
    }
    pub fn set_eq(&mut self, a: VfsId, b: VfsId) {
        self.0.entry(Self::key(a,b))
            .or_insert(SMatch::Eq(a,b,false));
    }

    pub fn set_super_shadowed(&mut self, sup: VfsId, sub: VfsId) {
        let e = self.0.entry(Self::key(sup,sub));
        match e {
            Entry::Vacant(e) => {
                e.insert(SMatch::Super(sup,sub,true));
            }
            Entry::Occupied(mut e) => {
                if let SMatch::Super(a,b,_) = e.get() {
                    if !(*a == sup && *b == sub) {
                        panic!();
                    }
                    e.insert(SMatch::Super(sup,sub,true));
                } else {
                    panic!();
                }
            }
        }
    }

    pub fn set_eq_shadowed(&mut self, a: VfsId, b: VfsId) {
        self.0.insert(
            Self::key(a,b),
            SMatch::Eq(a,b,true),
        );
    }

    fn key(a: VfsId, b: VfsId) -> (usize,usize) {
        let (a,b) = (a.evil_inner,b.evil_inner);
        if a > b {
            (b,a)
        } else {
            (a,b)
        }
    }
}

#[derive(Clone,Copy)]
pub enum SMatch {
    /// a, b, shadowed
    Eq(VfsId,VfsId,bool),
    // super, sub, shadowed
    Super(VfsId,VfsId,bool),
}

impl SMatch {
    pub fn is_shadowed(&self) -> bool {
        match *self {
            SMatch::Eq(_,_,s) => s,
            SMatch::Super(_,_,s) => s,
        }
    }
}

pub enum Ordr {
    Nope,
    Super,
    Sub,
    Eq,
}

pub fn all_to_all<T>(v: &[T], mut fun: impl FnMut(&T,&T)) {
    for i in 0..v.len() {
        let a = &v[i];
        for b in &v[i+1..] {
            fun(a,b);
        }
    }
}

pub fn sort_em(a: &mut Vfs) {
    for i in 0..a.entries.len() {
        let mut v = std::mem::take(&mut a.entries[i].childs);
        v.sort_by_key(|i| {
            let f = &a[*i];
            (f.is_dir,&*f.path,f.dir_size,f.file_size,f.dir_hash.as_ref(),f.file_hash.as_ref())
        });
        a.entries[i].childs = v;
    }
}

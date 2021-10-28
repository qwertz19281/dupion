use super::*;

pub struct TreeRoot<'a> {
    state: &'a State,
    roots: Vec<VfsId>,
    force_absolute_paths: bool,
}
pub struct DirEntry<'a> {
    state: &'a State,
    id: VfsId,
    roots_idx: usize,
    roots: &'a [VfsId],
    force_absolute_paths: bool,
}

impl<'a> Serialize for TreeRoot<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let iter = self.roots.iter()
            .enumerate()
            .filter(|(_,id)| self.state.tree[**id].exists() )
            .map(|(idx,id)| {
                let path = &self.state.tree[*id].path;
                let name = reduce_path(path,path,true);
                (name,DirEntry{
                    state: self.state,
                    id: *id,
                    roots_idx: idx,
                    roots: &self.roots,
                    force_absolute_paths: self.force_absolute_paths,
                })
            });
        serializer.collect_map(iter)
    }
}

impl<'a> Serialize for DirEntry<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        assert!(self.state.tree[self.id].exists());
        let mut childion = self.state.tree[self.id].childs.iter()
            .filter(|&&cid| self.state.tree[cid].exists() )
            .map(|&cid| {
                let dup = self.state.tree[cid].treediff_stat;
                //eprintln!("{}",self.state.tree[cid].path.to_str().unwrap());
                let name = self.state.tree[cid].path.file_name().unwrap().to_str().unwrap();
                let size = self.state.tree[cid].file_or_dir_props().0.unwrap_or(0);
                let ip = self.state.tree[cid].icon_prio2();
                (dup,cid,size,ip,name)
            })
            .collect::<Vec<_>>();
        
        childion.sort_by_key(|v| (v.3,Reverse(v.0),Reverse(v.2),v.4) );

        let iter = childion.iter()
            .map(|(dups,id,size,_,name)| {
                let e = &self.state.tree[*id];
                let icon = e.icon3();
                if *dups == 2 {
                    let ident = format!("DUPS {} {} {}",icon,name,size);
                    //let hash = self.state.tree[*id].file_or_dir_props().1.unwrap();
                    (ident,None)
                }else{
                    let ident = match *dups {
                        1 => format!("SUPR {} {} {}",icon,name,size),
                        0 => format!("UNIQ {} {} {}",icon,name,size),
                        _ => panic!(),
                    };
                    (ident,Some(DirEntry{
                        state: self.state,
                        id: *id,
                        roots_idx: self.roots_idx,
                        roots: self.roots,
                        force_absolute_paths: self.force_absolute_paths,
                    }))
                }
            });
        serializer.collect_map(iter)
    }
}

pub fn find_diffs(state: &mut State, roots: &[VfsId]) {
    for roots_idx in 0..roots.len() {
        for (i,id) in roots.iter().enumerate() {
            if i != roots_idx {
                find_diff(state, roots[roots_idx], roots[roots_idx], *id);
            }
        }
    }
    //found
}
pub fn find_diff(state: &mut State, inner: VfsId, root: VfsId, other_root: VfsId) -> u8 {
    let innerp = &state.tree[inner].path;
    let rootsrcp = &state.tree[root].path;
    let rootdstp = &state.tree[other_root].path;

    let montage = innerp.strip_prefix(rootsrcp).unwrap();
    let montage = rootdstp.join(montage);

    let mut eq = false;

    let e = &state.tree[inner];

    if let Some(f) = state.tree.resolve(&montage) {
        if let (Some(a),Some(b)) = (&e.file_hash,&f.file_hash) {
            eq = a == b;
        }else if let (Some(a),Some(b)) = (&e.dir_hash,&f.dir_hash) {
            eq = a == b;
        }
    }
    
    let upgrade = if eq {
        2
    }else{
        let mut uniqfree = state.tree[inner].is_dir;
        for c in state.tree[inner].childs.clone() {
            uniqfree &= find_diff(state, c, root, other_root) != 0;
        }
        if uniqfree {
            1
        }else{
            0
        }
    };
    state.tree[inner].treediff_stat = state.tree[inner].treediff_stat.max(upgrade);
    state.tree[inner].treediff_stat
}

pub fn reduce_path<'a>(path: &'a Path, root_path: &Path, force_absolute_paths: bool) -> &'a str {
    if !force_absolute_paths {
        path.strip_prefix(root_path).unwrap_or(path).to_str().unwrap()
    }else{
        path.to_str().unwrap()
    }
}

pub fn print_treediff(state: &mut State, opts: &Opts) {
    let roots = opts.paths.iter()
        .map(|p| state.tree.cid(p).unwrap() )
        .collect::<Vec<_>>();

    find_diffs(state, &roots);
    
    let ser = TreeRoot{
        state,
        roots,
        force_absolute_paths: opts.force_absolute_paths,
    };

    let mut stdout = std::io::stdout();

    serde_json::to_writer_pretty(&mut stdout, &ser).unwrap();

    stdout.flush().unwrap();
}

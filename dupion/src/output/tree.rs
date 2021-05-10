use super::*;

pub struct TreeRoot<'a> {
    state: &'a State,
    roots: Vec<VfsId>,
    force_absolute_paths: bool,
}
pub struct DirEntry<'a> {
    state: &'a State,
    id: VfsId,
    root_path: &'a Path,
    force_absolute_paths: bool,
}
pub struct Dupes<'a> {
    state: &'a State,
    group: &'a HashGroup,
    root_path: &'a Path,
    force_absolute_paths: bool,
}

pub enum DEnum<'a> {
    E(DirEntry<'a>),
    D(Dupes<'a>),
}

impl<'a> Serialize for TreeRoot<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let iter = self.roots.iter()
            .filter(|&&id| self.state.tree[id].exists() )
            .map(|&id| {
                let path = &self.state.tree[id].path;
                let name = reduce_path(path,path,true);
                (name,DirEntry{
                    state: self.state,
                    id: id,
                    root_path: path,
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
                let (s,h) = self.state.tree[cid].file_or_dir_props();
                let mut dups = 0;
                if let Some(h) = h {
                    dups = self.state.num_hashes(&h);
                    assert!(dups != 0);
                }
                //eprintln!("{}",self.state.tree[cid].path.to_str().unwrap());
                let ip = self.state.tree[cid].icon_prio2();
                (dups,cid,s.unwrap_or(0),ip)
            })
            .collect::<Vec<_>>();
        
        childion.sort_by_key(|v| (v.3,Reverse(v.0.max(1).min(2)),Reverse(v.2)) );

        let iter = childion.iter()
            .map(|(dups,id,size,_)| {
                let e = &self.state.tree[*id];
                let icon = e.icon3();
                let name = e.path.file_name().unwrap().to_str().unwrap();
                if *dups > 1 {
                    let ident = format!("DUPS {} {} {}",icon,name,size);
                    let hash = self.state.tree[*id].file_or_dir_props().1.unwrap();
                    (ident,DEnum::D(Dupes{
                        state: self.state,
                        group: &self.state.hashes[&hash],
                        root_path: self.root_path,
                        force_absolute_paths: self.force_absolute_paths,
                    }))
                }else{
                    let ident = format!("UNIQ {} {} {}",icon,name,size);
                    (ident,DEnum::E(DirEntry{
                        state: self.state,
                        id: *id,
                        root_path: self.root_path,
                        force_absolute_paths: self.force_absolute_paths,
                    }))
                }
            });
        serializer.collect_map(iter)
    }
}

impl<'a> Serialize for Dupes<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let iter = self.group.entries.iter()
            .take(4)
            .map(|(typ,id)| {
                let e = &self.state.tree[*id];
                let icon = typ.icon2(e.is_dir);
                let path = reduce_path(&e.path,self.root_path,self.force_absolute_paths);
                (format!("{} {}",icon,path),' ')
            });
        serializer.collect_map(iter)
    }
}

impl<'a> Serialize for DEnum<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::E(v) => v.serialize(serializer),
            Self::D(v) => v.serialize(serializer),
        }
    }
}

pub fn reduce_path<'a>(path: &'a Path, root_path: &Path, force_absolute_paths: bool) -> &'a str {
    if !force_absolute_paths {
        path.strip_prefix(root_path).unwrap_or(path).to_str().unwrap()
    }else{
        path.to_str().unwrap()
    }
}

pub fn print_tree(state: &State, opts: &Opts) {
    let roots = opts.paths.iter()
        .map(|p| state.tree.cid(p).unwrap() )
        .collect::<Vec<_>>();
    
    let ser = TreeRoot{
        state: state,
        roots,
        force_absolute_paths: opts.force_absolute_paths,
    };

    let mut stdout = std::io::stdout();

    serde_json::to_writer_pretty(&mut stdout, &ser).unwrap();

    stdout.flush().unwrap();
}

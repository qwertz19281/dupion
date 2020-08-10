use super::*;
use size_format::SizeFormatterBinary;

pub fn print_groups(v: &[HashGroup], b: &State, opts: &Opts) {
    for h in v {
        let mut non_shadowed = 0usize;
        let mut shadowed = 0usize;

        let entries = &h.entries.iter()
            .filter(|(typ,e)| b.tree[*e].is2(*typ) )
            .collect::<Vec<_>>();

        if entries.len() <= 1 {continue;}

        for (typ,e) in entries.iter() {
            let e = &b.tree[*e];
            if e.exists() {
                if e.shadowed(*typ) {
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
        for (typ,e) in entries {
            let e = &b.tree[*e];
            let shadowed = e.shadowed(*typ);
            if !hide_shadowed || !shadowed {
                assert_eq!(e.size(*typ).unwrap(),h.size);
                let tt = typ.icon2(e.is_dir);
                println!(
                    "   {}{} {}",
                    tt,
                    if shadowed {'S'} else {' '},
                    opts.path_disp(&e.path)
                );
            }
        }
    }
}

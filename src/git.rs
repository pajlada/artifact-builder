// Most code based off of https://github.com/rust-lang/git2-rs/blob/master/examples/pull.rs

use anyhow::anyhow;
use git2::{BranchType, Repository};

#[allow(unused)]
use tracing::log::*;

pub fn fetch<'a>(
    repo: &'a git2::Repository,
    remote: &'a mut git2::Remote,
) -> Result<git2::AnnotatedCommit<'a>, git2::Error> {
    let mut fo = git2::FetchOptions::new();
    fo.download_tags(git2::AutotagOption::All);

    remote.fetch(&[] as &[&str], Some(&mut fo), None)?;

    let local_branch = repo.find_branch("master", BranchType::Local)?;
    let upstream_branch = local_branch.upstream()?;
    let upstream_commit = upstream_branch.into_reference().peel_to_commit()?;

    let upstream_annotated = repo.find_annotated_commit(upstream_commit.id())?;

    Ok(upstream_annotated)
}

fn fast_forward(
    repo: &Repository,
    lb: &mut git2::Reference,
    rc: &git2::AnnotatedCommit,
) -> Result<(), git2::Error> {
    let name = match lb.name() {
        Some(s) => s.to_string(),
        None => String::from_utf8_lossy(lb.name_bytes()).to_string(),
    };
    let msg = format!("Fast-Forward: Setting {} to id: {}", name, rc.id());
    println!("{}", msg);
    lb.set_target(rc.id(), &msg)?;
    repo.set_head(&name)?;
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::default()
            // For some reason the force is required to make the working directory actually get updated
            // I suspect we should be adding some logic to handle dirty working directory states
            // but this is just an example so maybe not.
            .force(),
    ))?;
    Ok(())
}

pub fn merge<'a>(
    repo: &'a Repository,
    remote_branch: &str,
    fetch_commit: git2::AnnotatedCommit<'a>,
) -> anyhow::Result<()> {
    // 1. do a merge analysis
    let analysis = repo.merge_analysis(&[&fetch_commit])?;

    // 2. Do the appropriate merge
    if analysis.0.is_fast_forward() {
        println!("Doing a fast forward");
        // do a fast forward
        let refname = format!("refs/heads/{}", remote_branch);
        match repo.find_reference(&refname) {
            Ok(mut r) => {
                fast_forward(repo, &mut r, &fetch_commit)?;
            }
            Err(_) => {
                // The branch doesn't exist so just set the reference to the
                // commit directly. Usually this is because you are pulling
                // into an empty repository.
                repo.reference(
                    &refname,
                    fetch_commit.id(),
                    true,
                    &format!("Setting {} to {}", remote_branch, fetch_commit.id()),
                )?;
                repo.set_head(&refname)?;
                repo.checkout_head(Some(
                    git2::build::CheckoutBuilder::default()
                        .allow_conflicts(true)
                        .conflict_style_merge(true)
                        .force(),
                ))?;
            }
        };
    } else if analysis.0.is_normal() {
        return Err(anyhow!(
            "Unable to merge upstream changes to local, no appropriate merge strategy found"
        ));
    } else {
        println!("Nothing to do...");
    }
    Ok(())
}

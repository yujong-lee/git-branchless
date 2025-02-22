use branchless::testing::{
    make_git, make_git_with_remote_repo, GitInitOptions, GitRunOptions, GitWrapperWithRemoteRepo,
};

#[test]
fn test_init_smartlog() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;

    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @"@ f777ecc9 (> master) create initial.txt
");
    }

    Ok(())
}

#[test]
fn test_show_reachable_commit() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.run(&["checkout", "-b", "initial-branch", "master"])?;
    git.commit_file("test", 1)?;

    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
        O f777ecc9 (master) create initial.txt
        |
        @ 3df4b935 (> initial-branch) create test.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_tree() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.detach_head()?;
    git.run(&["branch", "initial"])?;
    git.commit_file("test1", 1)?;
    git.run(&["checkout", "initial"])?;
    git.commit_file("test2", 2)?;

    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
        O f777ecc9 (master) create initial.txt
        |\
        | o 62fc20d2 create test1.txt
        |
        @ fe65c1fe (> initial) create test2.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_rebase() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.run(&["checkout", "-b", "test1", "master"])?;
    git.commit_file("test1", 1)?;
    git.run(&["checkout", "master"])?;
    git.detach_head()?;
    git.commit_file("test2", 2)?;
    git.run(&["rebase", "test1"])?;

    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
            O f777ecc9 (master) create initial.txt
            |
            o 62fc20d2 (test1) create test1.txt
            |
            @ f8d9985b create test2.txt
            "###);
    }

    Ok(())
}

#[test]
fn test_sequential_master_commits() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.commit_file("test1", 1)?;
    git.commit_file("test2", 2)?;
    git.commit_file("test3", 3)?;

    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
        :
        @ 70deb1e2 (> master) create test3.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_merge_commit() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.run(&["checkout", "-b", "test1", "master"])?;
    git.commit_file("test1", 1)?;
    git.run(&["checkout", "-b", "test2and3", "master"])?;
    git.commit_file("test2", 2)?;
    git.commit_file("test3", 3)?;
    git.run_with_options(
        &["merge", "test1"],
        &GitRunOptions {
            time: 4,
            ..Default::default()
        },
    )?;

    {
        // Rendering here is arbitrary and open to change.
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
        O f777ecc9 (master) create initial.txt
        |\
        | o 62fc20d2 (test1) create test1.txt
        | |
        | @ fa4e4e1a (> test2and3) Merge branch 'test1' into test2and3
        |
        o fe65c1fe create test2.txt
        |
        o 02067177 create test3.txt
        |
        @ fa4e4e1a (> test2and3) Merge branch 'test1' into test2and3
        "###);
    }

    Ok(())
}

#[test]
fn test_rebase_conflict() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.run(&["checkout", "-b", "branch1", "master"])?;
    git.commit_file_with_contents("test", 1, "contents 1\n")?;
    git.run(&["checkout", "-b", "branch2", "master"])?;
    git.commit_file_with_contents("test", 2, "contents 2\n")?;

    // Should produce a conflict.
    git.run_with_options(
        &["rebase", "branch1"],
        &GitRunOptions {
            expected_exit_code: 1,
            ..Default::default()
        },
    )?;
    git.resolve_file("test", "contents resolved\n")?;
    git.run(&["rebase", "--continue"])?;

    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
        O f777ecc9 (master) create initial.txt
        |
        o 88646b56 (branch1) create test.txt
        |
        @ 4549af33 (> branch2) create test.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_non_adjacent_commits() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.detach_head()?;
    git.commit_file("test1", 1)?;
    git.run(&["checkout", "master"])?;
    git.commit_file("test2", 2)?;
    git.commit_file("test3", 3)?;
    git.detach_head()?;
    git.commit_file("test4", 4)?;

    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
            O f777ecc9 create initial.txt
            |\
            : o 62fc20d2 create test1.txt
            :
            O 02067177 (master) create test3.txt
            |
            @ 8e62740b create test4.txt
            "###);
    }

    Ok(())
}

#[test]
fn test_non_adjacent_commits2() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.detach_head()?;
    git.commit_file("test1", 1)?;
    git.commit_file("test2", 2)?;
    git.run(&["checkout", "master"])?;
    git.commit_file("test3", 3)?;
    git.commit_file("test4", 4)?;
    git.detach_head()?;
    git.commit_file("test5", 5)?;

    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
            O f777ecc9 create initial.txt
            |\
            : o 62fc20d2 create test1.txt
            : |
            : o 96d1c37a create test2.txt
            :
            O 2b633ed7 (master) create test4.txt
            |
            @ 13932989 create test5.txt
            "###);
    }

    Ok(())
}

#[test]
fn test_non_adjacent_commits3() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.commit_file("test1", 1)?;
    git.detach_head()?;
    git.commit_file("test2", 2)?;
    git.run(&["checkout", "master"])?;
    git.commit_file("test3", 3)?;
    git.detach_head()?;
    git.commit_file("test4", 4)?;
    git.run(&["checkout", "master"])?;
    git.commit_file("test5", 5)?;
    git.commit_file("test6", 6)?;

    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
        :
        O 62fc20d2 create test1.txt
        |\
        | o 96d1c37a create test2.txt
        |
        O 4838e49b create test3.txt
        |\
        : o a2482074 create test4.txt
        :
        @ 500c9b3e (> master) create test6.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_custom_main_branch() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.run(&["branch", "-m", "master", "main"])?;
    git.run(&["config", "branchless.core.mainBranch", "main"])?;
    git.commit_file("test1", 1)?;
    git.detach_head()?;
    git.commit_file("test2", 2)?;

    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
            :
            O 62fc20d2 (main) create test1.txt
            |
            @ 96d1c37a create test2.txt
            "###);
    }

    Ok(())
}

#[test]
fn test_main_remote_branch() -> eyre::Result<()> {
    let GitWrapperWithRemoteRepo {
        temp_dir: _guard,
        original_repo,
        cloned_repo,
    } = make_git_with_remote_repo()?;

    {
        original_repo.init_repo()?;
        original_repo.commit_file("test1", 1)?;
        original_repo.run(&[
            "clone",
            original_repo.repo_path.to_str().unwrap(),
            cloned_repo.repo_path.to_str().unwrap(),
        ])?;
    }

    {
        cloned_repo.init_repo_with_options(&GitInitOptions {
            make_initial_commit: false,
            ..Default::default()
        })?;
        cloned_repo.detach_head()?;
        cloned_repo.run(&["config", "branchless.core.mainBranch", "origin/master"])?;
        cloned_repo.run(&["branch", "-d", "master"])?;
        let (stdout, _stderr) = cloned_repo.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
        :
        @ 62fc20d2 (remote origin/master) create test1.txt
        "###);
    }

    {
        original_repo.commit_file("test2", 2)?;
    }

    {
        cloned_repo.run(&["fetch"])?;
        let (stdout, _stderr) = cloned_repo.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
        :
        @ 62fc20d2 create test1.txt
        |
        O 96d1c37a (remote origin/master) create test2.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_show_rewritten_commit_hash() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.commit_file("test1", 1)?;
    git.commit_file("test2", 2)?;
    git.run(&["prev"])?;
    git.run(&["commit", "--amend", "-m", "test1 version 1"])?;
    git.run(&["commit", "--amend", "-m", "test1 version 2"])?;

    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
            O f777ecc9 create initial.txt
            |\
            | @ 2ebe0950 test1 version 2
            |
            X 62fc20d2 (rewritten as 2ebe0950) create test1.txt
            |
            O 96d1c37a (master) create test2.txt
            "###);
    }

    Ok(())
}

#[test]
fn test_smartlog_orphaned_root() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.commit_file("test1", 1)?;

    git.run(&["checkout", "--orphan", "new-root"])?;

    {
        let (stdout, stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stderr, @"");
        insta::assert_snapshot!(stdout, @r###"
        :
        O 62fc20d2 (master) create test1.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_show_hidden_commits() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.commit_file("test1", 1)?;
    git.detach_head()?;
    git.commit_file("test2", 2)?;
    git.run(&["commit", "--amend", "-m", "amended test2"])?;
    git.run(&["hide", "HEAD"])?;
    git.run(&["checkout", "HEAD^"])?;

    {
        let (stdout, stderr) = git.run(&["smartlog", "--hidden"])?;
        insta::assert_snapshot!(stderr, @"");
        insta::assert_snapshot!(stdout, @r###"
        :
        @ 62fc20d2 (master) create test1.txt
        |\
        | x cb8137ad (manually hidden) amended test2
        |
        x 96d1c37a (rewritten as cb8137ad) create test2.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_show_only_branches() -> eyre::Result<()> {
    let git = make_git()?;

    git.init_repo()?;
    git.commit_file("test1", 1)?;
    git.detach_head()?;
    let test2_oid = git.commit_file("test2", 2)?;
    git.run(&["checkout", "master"])?;
    git.commit_file("test3", 3)?;
    git.detach_head()?;
    let test4_oid = git.commit_file("test4", 4)?;
    git.run(&["checkout", "master"])?;
    git.commit_file("test5", 5)?;
    git.detach_head()?;
    let test6_oid = git.commit_file("test6", 6)?;
    git.run(&["checkout", "master"])?;
    git.commit_file("test7", 7)?;
    git.detach_head()?;
    git.commit_file("test8", 8)?;
    git.run(&["checkout", "master"])?;
    git.commit_file("test9", 9)?;

    git.run(&["branch", "branch-2", &test2_oid.to_string()])?;
    git.run(&["branch", "branch-4", &test4_oid.to_string()])?;
    git.run(&["hide", &test4_oid.to_string()])?;
    git.run(&["hide", &test6_oid.to_string()])?;

    // confirm our baseline:
    // branch, hidden branch and non-branch head are visible; hidden non-branch head is not
    {
        let (stdout, _stderr) = git.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
        :
        O 62fc20d2 create test1.txt
        |\
        | o 96d1c37a (branch-2) create test2.txt
        |
        O 4838e49b create test3.txt
        |\
        : x a2482074 (manually hidden) (branch-4) create test4.txt
        :
        O 8577a964 create test7.txt
        |\
        | o e8b6a382 create test8.txt
        |
        @ 1b854edc (> master) create test9.txt
        "###);
    }

    // just branches (normal and hidden) but no non-branch heads
    {
        let (stdout, _stderr) = git.run(&["smartlog", "--only-branches"])?;
        insta::assert_snapshot!(stdout, @r###"
        :
        O 62fc20d2 create test1.txt
        |\
        | o 96d1c37a (branch-2) create test2.txt
        |
        O 4838e49b create test3.txt
        |\
        : x a2482074 (manually hidden) (branch-4) create test4.txt
        :
        @ 1b854edc (> master) create test9.txt
        "###);
    }

    Ok(())
}

#[test]
fn test_active_non_head_main_branch_commit() -> eyre::Result<()> {
    let GitWrapperWithRemoteRepo {
        temp_dir: _guard,
        original_repo,
        cloned_repo,
    } = make_git_with_remote_repo()?;

    let test1_oid = {
        original_repo.init_repo()?;
        let test1_oid = original_repo.commit_file("test1", 1)?;
        original_repo.commit_file("test2", 2)?;
        original_repo.commit_file("test3", 3)?;

        original_repo.clone_repo_into(&cloned_repo, &[])?;

        test1_oid
    };

    {
        cloned_repo.init_repo_with_options(&GitInitOptions {
            make_initial_commit: false,
            ..Default::default()
        })?;
        // Ensure that the `test1` commit isn't visible just because it's been
        // un-hidden. It's a public commit, so it should be hidden if possible.
        cloned_repo.run(&["unhide", &test1_oid.to_string()])?;

        let (stdout, _stderr) = cloned_repo.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
        :
        @ 70deb1e2 (> master, remote origin/master) create test3.txt
        "###);
    }

    {
        // Verify that both `origin/master` and `master` appear in the smartlog.
        cloned_repo.commit_file("test4", 4)?;
        let (stdout, _stderr) = cloned_repo.run(&["smartlog"])?;
        insta::assert_snapshot!(stdout, @r###"
        :
        O 70deb1e2 (remote origin/master) create test3.txt
        |
        @ 355e173b (> master) create test4.txt
        "###);
    }

    Ok(())
}

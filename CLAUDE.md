# mic-rtsim

## Landing

Every branch lands through a pull request, so that Graphide's review runs first. You still land it
yourself.

1. Finish the branch as usual: merge `origin/main`, check that `git diff --name-only --diff-filter=U`
   prints nothing (any unmerged path, not only `UU`), and run the tests the change needs.
2. Open the pull request and wait for the review, as a background task:

   ```
   /home/greencheetah/Projects/graphide/graphide/utilities/scripts/pr-review.sh \
     --root <worktree> --base main --body-file <file>
   ```

   It pushes the branch, opens the pull request (or finds the open one), waits for the "Graphide
   review" check run on the pushed head, and prints the verdict and the findings. End the body file
   with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
3. Act on its exit status:
   - 0: read the verdict and every finding. Fix the real ones (a bug gets a failing test first),
     push, and run the script again, so the fix gets its own round. Answer each false positive in
     the pull request with a one-line reason.
   - 2: git or gh failed. Fix the cause and run it again. Do not land.
   - 3: no review started. Land without one and say so. After a fix push this means the fix landed
     unreviewed: say that too.
   - 4: the review is still running. Run the script again; it keeps waiting on the same pull
     request. Only after a second 4, land without the review and say so: landing closes the pull
     request and ends that review.
4. Land. If `main` moved, merge it again, check for unmerged paths, and rerun the tests for any code
   the merge brought in. Then `git push origin HEAD:main`. A clean merge of `main` needs no new
   review round. Apart from the second 4 above, never land while a review is still running.
5. Delete the remote branch: `git push origin --delete <branch>`.

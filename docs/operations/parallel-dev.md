Here’s the **full updated manual**, matching your structure and workflow preferences: root “meta” directory, nested repo directory, sibling `worktrees/`, agents named **`a`** and **`b`**, and **no integrate** branch/worktree (all merges land in `main`).

---

# Manual: Parallel development with Git worktrees (agents `a` and `b`)

## 1) Target filesystem layout

You want a *wrapper* directory that contains:

* a README (human/operator notes),
* the actual Git repository in a subfolder,
* and a `worktrees/` folder for additional worktrees.

```
<project-name>/                  # wrapper (NOT the repo)
├── README.md                    # describes what's happening here
├── <project-name>/              # the actual git repo (main worktree)
│   ├── .git/
│   ├── src/ ...
│   └── ...
└── worktrees/                   # additional worktrees
    ├── a/                       # agent A worktree
    └── b/                       # agent B worktree
```

This layout is excellent because it keeps:

* the “control plane” (`README.md`, conventions) separate from the repo,
* worktrees as clean siblings of the main worktree (so tooling/IDEs don’t recursively index nonsense).

---

## 2) Core idea (what Git enforces)

* Each worktree is an additional working directory connected to the same repository data.
* **A given branch can only be checked out in one worktree at a time.**
* Therefore, your “three streams” become:

  * `<project-name>/<project-name>` on `main`
  * `<project-name>/worktrees/a` on some agent branch (or task branch)
  * `<project-name>/worktrees/b` on some agent branch (or task branch)

---

## 3) Initial setup (one-time)

### 3.1 Create wrapper + clone the repo inside it

From wherever you keep projects:

```bash
mkdir <project-name>
cd <project-name>

git clone <git-url> <project-name>
mkdir -p worktrees
```

Now your main worktree is `<project-name>/<project-name>`.

### 3.2 Add agent worktrees (`a` and `b`)

Run these from inside the main worktree:

```bash
cd <project-name>

git fetch origin

git worktree add ../worktrees/a -b agent/a origin/main
git worktree add ../worktrees/b -b agent/b origin/main
```

You now have:

* `../worktrees/a` checked out on `agent/a`
* `../worktrees/b` checked out on `agent/b`

---

## 4) Daily workflow (parallel agents)

### 4.1 Agent A works in `worktrees/a`

```bash
cd <project-name>/worktrees/a
# edit, test, commit
git status
git commit -am "Implement X"
git push -u origin agent/a
```

### 4.2 Agent B works in `worktrees/b`

```bash
cd <project-name>/worktrees/b
# edit, test, commit
git commit -am "Fix Y"
git push -u origin agent/b
```

### 4.3 Merge into `main` from the main worktree only (your preference)

```bash
cd <project-name>/<project-name>

git fetch origin
git switch main
git merge --no-ff origin/agent/a
git merge --no-ff origin/agent/b

git push origin main
```

(If you prefer PRs, the mechanics are identical; the merge happens via your platform.)

---

## 5) Persistence: keep `a/` and `b/` worktrees without re-setup

You can absolutely keep those directories persistent to avoid reinstalling dependencies, rebuilding toolchains, etc. Two good patterns exist; pick the one that matches your habits.

### Pattern A — Long-lived agent branches (`agent/a`, `agent/b`)

Your directories stay permanently on those branches. For each new task, you “refresh” the branch from `main`.

**Refresh by rebasing (keeps history linear, preserves commits):**

```bash
cd <project-name>/worktrees/a
git fetch origin
git switch agent/a
git rebase origin/main
```

**Refresh by hard reset (clean slate; discards local commits unless pushed):**

```bash
cd <project-name>/worktrees/a
git fetch origin
git switch agent/a
git reset --hard origin/main
```

Use **rebase** if you want to keep work-in-progress commits; use **reset** if each task should start from a pristine `main`.

> If you pushed commits and then reset/rewrite, you may need `git push --force-with-lease` for that agent branch. That’s safe-ish if only you/your agent uses it.

### Pattern B — Persistent directories, but per-task branches inside them

Here the directory stays, but you create a fresh branch per task in that directory:

```bash
cd <project-name>/worktrees/a
git fetch origin
git switch -c task/a-<short-name> origin/main
```

Work, push, merge, then delete that task branch when done.

After merge, you can switch `a/` back to a “parking” branch:

```bash
git switch agent/a
git reset --hard origin/main
```

This gives you:

* stable environments (directory persists),
* clean branch semantics (each task has its own branch),
* and no need to recreate worktrees.

---

## 6) “Change pointing branches” on a worktree: what’s possible?

Yes—**a worktree can switch branches** like any normal checkout, with one key constraint:

* You **cannot** switch `worktrees/a` onto a branch already checked out elsewhere (e.g. `main`, since it’s checked out in `<project-name>/<project-name>`).

So:

* Switching `worktrees/a` between `agent/a` and `task/a-*` is fine.
* Switching `worktrees/a` onto `main` will fail (because `main` is already in the main worktree).

This is exactly why your three-stream model works so smoothly.

---

## 7) Updating all streams from `main` (your “3 streams” routine)

When `main` advances and you want both agent worktrees refreshed:

```bash
cd <project-name>/<project-name>
git fetch origin
git pull --ff-only
```

Then for each agent:

```bash
cd <project-name>/worktrees/a
git fetch origin
git rebase origin/main   # or reset --hard origin/main
```

```bash
cd <project-name>/worktrees/b
git fetch origin
git rebase origin/main   # or reset --hard origin/main
```

---

## 8) Cleanup (only if you decide you *do* want recreation)

If a worktree gets messy, deletion is straightforward:

```bash
cd <project-name>/<project-name>
git worktree remove ../worktrees/a
git branch -D agent/a          # optional: delete the branch too
```

If you manually deleted folders and Git still thinks they exist:

```bash
git worktree prune
```

---

## 9) Operational cautions (the things that actually bite)

1. **One branch per worktree** (non-negotiable Git invariant).
2. **Port collisions**: if both worktrees run dev servers, assign per-worktree ports via `.env.local` (gitignored).
3. **Tooling caches**: most are fine shared; project-local artifacts are naturally per worktree because directories differ.
4. **Uncommitted changes**: before rebasing/resetting a persistent agent directory, either commit or stash.

---

## 10) README.md suggestion (for your wrapper root)

Keep a short note in `<project-name>/README.md` such as:

* what `<project-name>/<project-name>` is (main worktree, `main` branch),
* what `worktrees/a` and `worktrees/b` are (parallel agent sandboxes),
* branch naming rules (`agent/a`, `agent/b`, optional `task/a-*`),
* your chosen refresh method (rebase vs reset).

---

> [!IMPORTANT] This approach can be extended to more than two streams (a and b > a,b,c,d...).

git fetch origin

git switch main && git pull origin main

git switch refactor/tui && git merge main
git switch refactor/model && git merge main
git switch refactor/app && git merge main

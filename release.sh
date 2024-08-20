#!/bin/bash

# this script tags and pushes the tag on origin
# first thing, we need to read the latest tag from the repo
# then we need to increment it

# first check that there are no uncommitted changes
if [[ $(git status --porcelain) ]]; then
    echo "There are uncommitted changes, please commit them before proceeding"
    exit 1
fi

# check that we are on master/main
current_branch=$(git branch --show-current)
if [[ $current_branch != "master" && $current_branch != "main" ]]; then
    echo "You are not on master/main, are you sure you want to proceed?"
    read -p "Do you want to proceed? [y/n]" -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]
    then
        echo "OK"
    else
        echo "Aborting..."
        exit 1
    fi
fi

# get latest tag
latest_tag=$(git describe --abbrev=0 --tags)

# split the tag into its components
IFS='.' read -r -a tag_parts <<< "$latest_tag"


# strip away the v from the first part
major_version=${tag_parts[0]:1}
minor_version=${tag_parts[1]}
# increment the patch version
patch_version=$((${tag_parts[2]}+1))

new_tag=$major_version.$minor_version.$patch_version
# ask for confirmation
read -p "The latest tag is $latest_tag, do you want to increment it to v$new_tag? [y/n]" -n 1 -r
echo

if [[ $REPLY =~ ^[Yy]$ ]]
then
    echo "OK"
else
    # enter the new tag
    read -p "Enter the new tag in X.X.X format: " new_tag
    # check if format is valid 
    if [[ $new_tag =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
    then
        echo "New tag will be v$new_tag"

        # proceed?
        read -p "Do you want to proceed? [y/n]" -n 1 -r
        echo

        if [[ $REPLY =~ ^[Yy]$ ]]
        then
            echo "Proceeding..."
        else
            echo "Aborting..."
            exit 1
        fi

    else
        echo "Tag format is not valid, it should be major.minor.patch, e.g. 1.0.0"
        exit 1
    fi
fi

short_commit_hash=$(git rev-parse --short HEAD)
long_commit_hash=$(git rev-parse HEAD)

# replace version number in Cargo.toml
if [[ "$OSTYPE" == "darwin"* ]]; then
  # Mac OS
  sed -i '' 's/^version = \".*\"/version = \"'$new_tag'\"/' Cargo.toml
else
  # Linux
  sed -i 's/^version = \".*\"/version = \"'$new_tag'\"/' Cargo.toml
fi

# overwrite the file src/library/version.rs
echo "pub const FULL_VERSION: &str = \"v$new_tag-$short_commit_hash\";" > ./src/library/version.rs
TZ=UTC echo "pub const LONG_VERSION: &str = \"
version: v$new_tag
commit: $long_commit_hash
branch: $current_branch
released on: $(date -R)\";
" >> ./src/library/version.rs



# commit the changes
git add Cargo.toml src/library/version.rs

git commit -m "Bump version to v$new_tag"
git push
git tag v$new_tag
git push origin v$new_tag

echo "Tag v$new_tag has been pushed to origin"
echo "In order to trigger the release build, you need to remove the draft status from the release in GitHub"
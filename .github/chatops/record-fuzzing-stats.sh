#!/bin/bash

REPO=${1}
COMMENTID=${2:-all}
COMMENT=$3
ISSUEID=${4:-70}

parse_comment() {
    COMMENT=$1
    INPUT=$(echo "$COMMENT" | awk '{ print NF; exit }')

    [[ $(echo "$COMMENT" | awk '{ print $1 }') != "/fuzzed" ]] && echo "No magic command found" && return 1
    [[ $INPUT -lt 2 ]] && echo "Not enough inputs" && return 1

    LOOP=1
    while [ $LOOP -le $INPUT ]; do
        FIELD=$(echo "$COMMENT" | awk "{ print \$$LOOP }")
        COLUMN=$(echo "$FIELD" | awk -F'=' '{ print $1 }')
        DATA=$(echo "$FIELD" | awk -F'=' '{ print $2 }')

        [[ $COLUMN == "repo" ]] && REPO=$DATA
        [[ $COLUMN == "date" ]] && DATE=$DATA
        [[ $COLUMN == "solutions" ]] && SOLUTIONS=$DATA
        [[ $COLUMN == "runtime" ]] && RUNTIME=$DATA

        LOOP=$(( $LOOP + 1 )) || :
    done

    [[ -z $REPO ]] && return 1
    [[ -z $DATE ]] && return 1
    [[ -z $SOLUTIONS ]] && return 1
    [[ -z $RUNTIME ]] && return 1

    HDATE=$(date -d @$DATE)
    echo "${REPO},${DATE},${HDATE},${SOLUTIONS},${RUNTIME}"
    echo "${REPO},${DATE},${HDATE},${SOLUTIONS},${RUNTIME}" >> _data/log.csv

    return 0
}

process_comment() {
    REPO=$1
    COMMENTID=$2
    COMMENT=$(echo "$3" | tr -dc '[:alnum:]-/=. ')

    parse_comment "$COMMENT"

    gh api --method DELETE \
        -H "Accept: application/vnd.github+json" \
        -H "X-GitHub-Api-Version: 2022-11-28" \
        /repos/${REPO}/issues/comments/${COMMENTID}
}

###############################################################################

cd gh-pages

git config --global user.name '${{ github.actor }}'
git config --global user.email '${{ github.actor }}@github.com'

if [ "$COMMENTID" != "all" ]; then
    process_comment "$REPO" "$COMMENTID" "$COMMENT"
else
    echo "Get all comments"

    gh api \
        -H "Accept: application/vnd.github+json" \
        -H "X-GitHub-Api-Version: 2022-11-28" \
        /repos/${REPO}/issues/${ISSUEID}/comments |
    jq '.[]' | jq '. | "\(.id), \(.body)"' | tr -d '"' > data

    while IFS="," read -r COMMENTID COMMENT
    do
        process_comment "$REPO" "$COMMENTID" "$COMMENT"
    done < data
fi

git add _data/log.csv
git commit -m "Update stats"
git push
#!/bin/bash

WORKDIR="/github/workspace"
CORPUS="corpus"
SOLUTIONS="solutions"
CHECKPOINT="checkpoint"
START=1
STOP=2
TIMEOUT="3.0"
TIMELIMIT="1440"
RANDOMSEED=0

while getopts ":t:c:s:C:S:P:T:E:W:R:L:" o; do
    case "${o}" in
        t)
            TIMEOUT=${OPTARG}
            ;;
        c)
            CORPUS=${OPTARG}
            ;;
        s)
            SOLUTIONS=${OPTARG}
            ;;
        C)
            CHECKPOINT=${OPTARG}
            ;;
        S)
            START=${OPTARG}
            ;;
        P)
            STOP=${OPTARG}
            ;;
        T)
            TIMELIMIT=${OPTARG}
            ;;
        E)
            EXTRACMDS=${OPTARG}
            ;;
        W)
            WORKDIR=${OPTARG}
            ;;
        R)
            RANDOMSEED=${OPTARG}
            ;;
        L)
            LOGLEVEL=${OPTARG}
            ;;
    esac
done
shift $((OPTIND-1))

[[ ! -d "$WORKDIR/$CHECKPOINT" ]] && echo "Checkpoint at '$WORKDIR/$CHECKPOINT' not found" && exit 1
if [ ! -z $EXTRACMDS ]; then
    [[ ! -f "$WORKDIR/$EXTRACMDS" ]] && echo "Extra commands file at '$WORKDIR/$EXTRACMDS' not found" && exit 1

    EXTRACMDS="extra_cmds=\"$WORKDIR/$EXTRACMDS\""
fi
[[ ! -z $LOGLEVEL ]] && LOGLEVEL="loglevel=$LOGLEVEL"
[[ ! -z $RANDOMSEED ]] && RANDOMSEED=${RANDOMSEED^^}

echo "WORKDIR: $WORKDIR"
echo "CHECKPOINT: $CHECKPOINT"
echo "TIMEOUT: $TIMEOUT"
echo "TIMELIMIT: $TIMELIMIT"
echo "RANDOMSEED: $RANDOMSEED"
echo "EXTRACMDS: $EXTRACMDS"
echo "LOGLEVEL: $LOGLEVEL"

mkdir -p "$WORKDIR/$CORPUS"
mkdir -p "$WORKDIR/$SOLUTIONS"

ispm settings install-dir /workspace/simics
ispm projects $WORKDIR --create --ignore-existing-files --non-interactive \
    1000-latest \
    1030-latest \
    2096-latest \
    8112-latest \
    31337-latest

timeout --preserve-status "${TIMELIMIT}m" \
$WORKDIR/bin/simics \
    -no-gui --no-win --batch-mode \
    -q --no-copyright --no-upgrade-info \
    /tsffs/start-tsffs.simics \
    timeout="$TIMEOUT" corpus="$WORKDIR/$CORPUS" solutions="$WORKDIR/$SOLUTIONS" checkpoint="$WORKDIR/$CHECKPOINT" \
    start="$START" stop="$STOP" randomseed="$RANDOMSEED" \
    $EXTRACMDS $LOGLEVEL
sc=$?

if [ $sc -eq 143 ]; then
    sc=0
fi

exit $sc

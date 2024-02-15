#!/usr/bin/env fish

argparse 'c/count' 'f/fishpath' 'h/help' 'n/names' -- $argv

if set -q -- _flag_help
    echo 'usage: ./man_files.fish [-c/--count | -f/--fishpath | -h/--help]'
    echo '-c/--count        Print the number of files in the directory(s).'
    echo '-f/--fishpath     Include the fish man page directory.'
    echo '-h/--help         Print this help message.'
    echo '-n/--names        Print file names.'
    echo -e "\nnote: /usr/share/man/ is always included."
    return 0
end

set -l manpath_count 0
set -l fishpath_count 0

# regular man files
for d in (find ~/../usr/share/man/* -type d)
    for f in (find $d/* -type f)
        if set -q -- _flag_count
            set manpath_count (math $manpath_count + 1)
        else if set -q -- _flag_names
            echo (path basename $f)
        else
            echo $f
        end
    end
end

# fish-specific man files
if set -q -- _flag_fishpath
    for d in (find ~/../usr/share/fish/man/* -type d)
        for f in (find $d/* -type f)
            if set -q -- _flag_count
                set fishpath_count (math $fishpath_count + 1)
            else if set -q -- _flag_names
                echo (path basename $f)
            else
                echo $f
            end
        end
    end
end

if set -q -- _flag_count
    echo "../share/man/ contains $manpath_count files."
    if set -q -- _flag_fishpath
        echo "../share/fish/man/ contains $fishpath_count files."
    end
end

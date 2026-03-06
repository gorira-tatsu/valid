input="$(cat)"

if printf '%s' "$input" | grep -q "(declare-fun action_1 () Int)"; then
  if printf '%s' "$input" | grep -q "(get-value (action_0))"; then
    printf 'sat\n'
    printf '((action_0 0))\n'
    printf '((action_1 0))\n'
  else
    printf 'sat\n'
  fi
else
  printf 'unsat\n'
fi

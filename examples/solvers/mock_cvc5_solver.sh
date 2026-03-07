input="$(cat)"

if printf '%s' "$input" | grep -q "(declare-fun action_1 () Int)"; then
  if printf '%s' "$input" | grep -q "(get-value (action_0))"; then
    action_count="$(
      printf '%s' "$input" \
        | sed -n 's/.*(<= action_[0-9][0-9]* \([0-9][0-9]*\))).*/\1/p' \
        | head -n 1
    )"
    if [ -z "$action_count" ]; then
      action_count=0
    fi
    printf 'sat\n'
    printf '%s' "$input" \
      | sed -n 's/.*(get-value (\(action_[0-9][0-9]*\))).*/\1/p' \
      | while IFS= read -r symbol; do
          step="${symbol#action_}"
          value=$((step % (action_count + 1)))
          printf '((%s %s))\n' "$symbol" "$value"
        done
  else
    printf 'sat\n'
  fi
else
  printf 'unsat\n'
fi

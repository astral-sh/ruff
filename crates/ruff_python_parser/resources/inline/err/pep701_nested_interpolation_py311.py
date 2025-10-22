# parse_options: {"target-version": "3.11"}
# nested interpolations also need to be checked
f'{1: abcd "{'aa'}" }'
f'{1: abcd "{"\n"}" }'

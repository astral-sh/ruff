# before
{  # open
 key# key
 : # colon
 value# value
} # close
# after


{**d}

{**a, #  leading
** # middle
b # trailing
}

{
** # middle with single item
b
}

{
    # before
    ** # between
    b,
}

{
    **a # comment before preceeding node's comma
 ,
    # before
    ** # between
    b,
}

{}

{1:2,}

{1:2,
 3:4,}

{asdfsadfalsdkjfhalsdkjfhalskdjfhlaksjdfhlaskjdfhlaskjdfhlaksdjfh: 1, adsfadsflasdflasdfasdfasdasdf: 2}

mapping = {
    A: 0.25 * (10.0 / 12),
    B: 0.1 * (10.0 / 12),
    C: 0.1 * (10.0 / 12),
    D: 0.1 * (10.0 / 12),
}

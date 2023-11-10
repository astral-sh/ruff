# flags: --preview --minimum-version=3.10
match x:
    case "abcd" | "abcd" | "abcd" :
        pass
    case "abcd" | "abcd" | "abcd" | "abcd" | "abcd" | "abcd" | "abcd" | "abcd" | "abcd" | "abcd" | "abcd" | "abcd" | "abcd" | "abcd" | "abcd":
        pass
    case xxxxxxxxxxxxxxxxxxxxxxx:
        pass

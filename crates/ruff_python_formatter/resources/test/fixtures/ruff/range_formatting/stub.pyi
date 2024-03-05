# Don't collapse the ellipsis if only formatting the ellipsis line.
class Test:
    <RANGE_START>...<RANGE_END>

class Test2: <RANGE_START>pass<RANGE_END>

class Test3:    <RANGE_START>...<RANGE_END>

class Test4:
    # leading comment
    <RANGE_START>...<RANGE_END>
    # trailing comment


class Test4:
<RANGE_START>    ...<RANGE_END>

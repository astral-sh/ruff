# For set expression, the minimum binding power of star expression is bitwise or.

{(*x), y}
{*x in y, z}
{*not x, z}
{*x and y, z}
{*x or y, z}
{*x if True else y, z}
{*lambda x: x, z}
{*x := 2, z}